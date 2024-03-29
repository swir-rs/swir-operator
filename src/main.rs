#[macro_use]
extern crate log;
use std::fs::{self, File};

use futures::StreamExt;
use k8s_openapi::{
    api::{apps::v1::Deployment, core::v1::ConfigMap},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use std::io::Read;

use kube::{
    api::{DeleteParams, ListParams, Patch, PatchParams, PostParams},
    Api, Client,
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};

use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use std::collections::BTreeMap;
use tokio::time::Duration;

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Failed to patch SWIR deployment: {}", source))]
    SwirPatchingFailed {
        source: kube::Error,
        backtrace: Backtrace,
    },
    MissingObjectKey {
        name: &'static str,
        backtrace: Backtrace,
    },
    SerializationFailed {
        source: serde_json::Error,
        backtrace: Backtrace,
    },
    ConfigurationFailed {
        config: String,
        namespace: String,
        backtrace: Backtrace,
    },
    FolderConfigFailed {
        details: String,
        source: std::io::Error,
    },
    HttpConfigFailed {
        details: String,
        source: reqwest::Error,
    },
}

trait ConfigSource {
    fn get_config(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error>;
    fn get_otel(&self, _namespace: &str, _deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
        Ok(BTreeMap::new())
    }
    fn get_certs(&self, _: &str) -> Result<BTreeMap<String, String>, Error> {
        Ok(BTreeMap::new())
    }
}

struct FolderBasedConfigSource(String, String, String);
struct HttpBasedConfigSource(String);

impl ConfigSource for FolderBasedConfigSource {
    fn get_config(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
        let file_name = format!("{}/{}/{}-config.yaml", self.0, namespace, deployment_name);
        let mut f = File::open(&file_name).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut contents = BTreeMap::new();
        contents.insert(deployment_name.to_string(), buffer);
        Ok(contents)
    }

    fn get_certs(&self, namespace: &str) -> Result<BTreeMap<String, String>, Error> {
        let folder_name = format!("{}/{}", self.1, namespace);
        let mut contents = BTreeMap::<String, String>::new();
        let iter = fs::read_dir(folder_name.clone()).context(FolderConfigFailed {
            details: format!("Unable to read dir {}", folder_name.clone()),
        })?;
        for dir_entry in iter {
            let dir_entry = dir_entry.context(FolderConfigFailed {
                details: format!("Unable to open file in folder {}", &folder_name),
            })?;

            let file_name = dir_entry.file_name();
            let file_name = String::from(file_name.to_string_lossy());
            if !dir_entry.path().starts_with("..") {
                if let Ok(mut f) = File::open(&dir_entry.path()) {
                    let mut buffer = String::new();
                    if let Ok(_) = f.read_to_string(&mut buffer) {
                        contents.insert(file_name.to_string(), buffer);
                    } else {
                        warn!("Can't read {}", file_name);
                    }
                } else {
                    warn!("Can't read {}", file_name);
                }
            }
        }
        Ok(contents)
    }

    fn get_otel(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
        let file_name = format!("{}/{}/otel-agent-config.yaml", self.2, namespace);
        let mut f = File::open(&file_name).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut contents = BTreeMap::new();
        contents.insert(deployment_name.to_string(), buffer);
        Ok(contents)
    }
}

impl ConfigSource for HttpBasedConfigSource {
    fn get_config(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
        let url = format!("{}/{}/{}", self.0, namespace, deployment_name);
        let body = reqwest::blocking::get(&url)
            .context(HttpConfigFailed { details: url.clone() })?
            .text()
            .context(HttpConfigFailed { details: url.clone() })?;
        let mut contents = BTreeMap::new();
        contents.insert("content".to_string(), String::from(deployment_name));
        contents.insert(deployment_name.to_string(), body);
        Ok(contents)
    }
}

/// Controller triggers this whenever our main object or our children changed
async fn reconcile_swir_deployment(resource: Deployment, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let image = &ctx.get_ref().image;
    let config_source = &ctx.get_ref().config_source;
    let client = &ctx.get_ref().client.clone();
    let reconciller_action: Result<ReconcilerAction, Error> = Ok(ReconcilerAction {
        //requeue_after: Some(Duration::from_secs(300)),
        requeue_after: None,
    });

    if let Some(labels) = resource.metadata.labels {
        if let Some(swir_label) = labels.get("swir") {
            let name = resource.metadata.name.context(MissingObjectKey { name: ".metadata.name" }).unwrap();
            let namespace = resource.metadata.namespace.context(MissingObjectKey { name: ".metadata.namespace" }).unwrap();

            let uid = resource.metadata.uid.context(MissingObjectKey { name: ".metadata.uid" }).unwrap();
            info!("Resource {} {} {} {} ", swir_label, name, namespace, uid);

            let api: Api<Deployment> = Api::namespaced(client.clone(), &namespace);

            let cm_api = Api::<ConfigMap>::namespaced(client.clone(), &namespace);

            let cm_cfg_name = format!("{}-config", swir_label);
            let cm_certs_name = format!("{}-certs", swir_label);
            let cm_otel_name = format!("{}-otel", swir_label);
            let maybe_configs = (
                config_source.get_config(&namespace, &swir_label),
                config_source.get_certs(&namespace),
                config_source.get_otel(&namespace, &swir_label),
            );

            if let (Ok(contents), Ok(certs), Ok(otel)) = maybe_configs {
                let cm_otel = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(cm_otel_name.clone()),
                        namespace: Some(namespace.clone()),
                        ..ObjectMeta::default()
                    },
                    data: Some(otel),
                    ..Default::default()
                };

                let cm_config = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(cm_cfg_name.clone()),
                        namespace: Some(namespace.clone()),
                        ..ObjectMeta::default()
                    },
                    data: Some(contents),
                    ..Default::default()
                };

                let cm_certs = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(cm_certs_name.clone()),
                        namespace: Some(namespace.clone()),
                        ..ObjectMeta::default()
                    },
                    data: Some(certs.clone()),
                    ..Default::default()
                };

                if let Ok(_res) = cm_api.delete(&cm_cfg_name, &DeleteParams { ..Default::default() }).await {
                    debug!("Deleted {}", cm_cfg_name);
                }

                if let Ok(_res) = cm_api.delete(&cm_otel_name, &DeleteParams { ..Default::default() }).await {
                    debug!("Deleted {}", cm_otel_name);
                }

                if let Ok(_res) = cm_api.delete(&cm_certs_name, &DeleteParams { ..Default::default() }).await {
                    debug!("Deleted {}", cm_certs_name);
                }

                let result = (
                    cm_api.create(&PostParams { ..Default::default() }, &cm_config).await,
                    cm_api.create(&PostParams { ..Default::default() }, &cm_certs).await,
                    cm_api.create(&PostParams { ..Default::default() }, &cm_otel).await,
                );

                let patch_params = PatchParams {
                    dry_run: false,
                    force: false,
                    field_manager: None,
                };

                if let (Ok(_), Ok(_), Ok(_)) = result {
                    info!("Config map created for {}", cm_cfg_name);
                    info!("Config map created for {}", cm_certs_name);
                    let spec_json = serde_json::json!({
                        "spec":{
                            "template":{
                                "spec": {
                    "containers":[
                        {
                        "name":"swir",
                        "image":image,
                        "env":[
                            {
                            "name":"swir_config_file",
                            "value":"/swir_config/config.yaml"
                            }
                        ],
                        "volumeMounts": [
                            {
                            "name":"config-volume",
                            "mountPath":"/swir_config"
                            },
                            {
                            "name":"certs-volume",
                            "mountPath":"/certs"
                            },
                        ]
                        },
                        {
                        "name": "otel-agent",
                        "image": "otel/opentelemetry-collector",
                        "args": ["--config=/etc/otel/otel-agent-config.yaml"],
                        "volumeMounts":[
                            {
                            "name":"otel-volume",
                            "mountPath": "/etc/otel"
                            }
                        ]
                        }
                    ]
                                }
                            }
                        }
                        });

                    let mut volumes_json = serde_json::json!({
                    "spec":{
                                    "template":{
                        "spec": {
                            "volumes":[
                            {
                                "name":"config-volume",
                                "configMap":{
                                "name": cm_cfg_name,
                                "items":[
                                    {
                                    "key":swir_label,
                                    "path":"config.yaml"
                                    }
                                ]
                                }
                            },
                            {
                                "name":"certs-volume",
                                "configMap":{
                                "name": cm_certs_name,
                                "items":[
                                ]
                                }
                            },
                            {
                                "name":"otel-volume",
                                "configMap":{
                                "name": cm_otel_name,
                                "items":[
                                    {
                                    "key":swir_label,
                                    "path":"otel-agent-config.yaml"
                                    }
                                ]
                                }
                            }
                            ]
                        }
                                    }
                    }
                            });

                    let spec_patch = Patch::Strategic(&spec_json);

                    if let Some(a) = volumes_json["spec"]["template"]["spec"]["volumes"][1]["configMap"]["items"].as_array_mut() {
                        for key in certs.keys() {
                            a.push(serde_json::json!({ "key": key,"path":key}));
                        }
                    }

                    info!("Volumes json {} {}", name, volumes_json);

                    let volumes_patch = Patch::Strategic(&volumes_json);

                    match api.patch(&name, &patch_params, &volumes_patch).await.context(SwirPatchingFailed) {
                        Ok(_res) => {
                            info!("Patched volumes {} {}", name, namespace);
                            match api.patch(&name, &patch_params, &spec_patch).await.context(SwirPatchingFailed) {
                                Ok(_res) => {
                                    info!("Patched containers {} {}", name, namespace);
                                    reconciller_action
                                }
                                Err(err) => {
                                    warn!("Patching containers failed {:?}", err);
                                    Err(err)
                                }
                            }
                        }
                        Err(err) => {
                            warn!("Patching volumes failed {:?}", err);
                            Err(err)
                        }
                    }
                } else {
                    warn!("Unable to create config map {} {} {:?}", namespace, swir_label, result);
                    reconciller_action
                }
            } else {
                warn!("No config for {} {} {:?}", namespace, swir_label, maybe_configs);
                reconciller_action
            }
        } else {
            reconciller_action
        }
    } else {
        reconciller_action
    }
}

/// The controller triggers this on reconcile errors
fn error_policy(_error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(1)),
    }
}

// Data we want access to in error/reconcile calls
struct Data {
    client: Client,
    config_source: Box<dyn ConfigSource + Sync + Send>,
    image: String,
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    let image = if let Ok(image_ver) = std::env::var("SWIR_SIDECAR_IMAGE_VERSION") {
        image_ver
    } else {
        "swir/swir:v0.4.0".to_string()
    };
    std::env::set_var("RUST_LOG", "info,kube-runtime=info,kube=info,hyper=info,tower=info");
    env_logger::init();
    let client = Client::try_default().await.unwrap();
    let config_source = FolderBasedConfigSource("./configs".to_string(), "./certs".to_string(), "./otel".to_string());
    debug! {"Running "};
    let cmgs = Api::<Deployment>::all(client.clone());
    let cms = Api::<Deployment>::all(client.clone());
    let lp1 = ListParams::default().labels("swir");
    let lp2 = ListParams::default().labels("swir");

    Controller::new(cmgs, lp1)
        .owns(cms, lp2)
        .run(
            reconcile_swir_deployment,
            error_policy,
            Context::new(Data {
                image,
                client,
                config_source: Box::new(config_source),
            }),
        )
        .for_each(|res| async move {
            match res {
                Ok(_o) => {}
                Err(e) => warn!("reconcile failed: {}", e),
            }
        })
        .await;
    Ok(())
}
