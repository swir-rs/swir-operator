#[macro_use]
extern crate log;
use std::fs::File;

use futures::StreamExt;
use k8s_openapi::{
    api::{apps::v1::Deployment, core::v1::ConfigMap},
    apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference},
};
use std::io::Read;

use kube::{
    api::{DeleteParams, ListParams, PatchParams, PatchStrategy, PostParams},
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
    fn get(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error>;
}

struct FolderBasedConfigSource(String);
struct HttpBasedConfigSource(String);

impl ConfigSource for FolderBasedConfigSource {
    fn get(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
        let file_name = format!("{}/{}/{}", self.0, namespace, deployment_name);
        let mut f = File::open(&file_name).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).context(FolderConfigFailed { details: file_name.clone() })?;
        let mut contents = BTreeMap::new();
        contents.insert("content".to_string(), String::from(deployment_name));
        contents.insert(deployment_name.to_string(), buffer);
        Ok(contents)
    }
}

impl ConfigSource for HttpBasedConfigSource {
    fn get(&self, namespace: &str, deployment_name: &str) -> Result<BTreeMap<String, String>, Error> {
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
async fn reconcile(resource: Deployment, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let config_source = &ctx.get_ref().config_source;
    let client = ctx.get_ref().client.clone();
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
            let patch_params = PatchParams {
                patch_strategy: PatchStrategy::Strategic,
                dry_run: false,
                ..Default::default()
            };

            let cm_api = Api::<ConfigMap>::namespaced(client.clone(), &namespace);

            let cm_name = format!("{}", swir_label);
            if let Ok(cm_contents) = config_source.get(&namespace, &swir_label) {
                let cm = ConfigMap {
                    metadata: ObjectMeta {
                        name: Some(cm_name.clone()),
                        namespace: Some(namespace.clone()),
                        // owner_references: Some(vec![OwnerReference {
                        //     controller: Some(true),
                        //     ..OwnerReference::default()
                        // }]),
                        ..ObjectMeta::default()
                    },
                    data: Some(cm_contents),
                    ..Default::default()
                };
                if let Ok(_res) = cm_api.delete(&cm_name, &DeleteParams { ..Default::default() }).await {
                    debug!("Deleted {}", cm_name);
                }

                if let Ok(_res) = cm_api.create(&PostParams { ..Default::default() }, &cm).await {
                    let spec_patch = serde_json::to_vec(&serde_json::json!({
                    "spec":{
                        "template":{
                        "spec": {
                            "containers":[
                            {
                                "name":"swir",
                                "image":"swir/swir:v3",
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
                                }
                                ]
                            }
                            ]
                        }
                        }
                    }
                     }))
                    .unwrap();

                    let volumes_patch = serde_json::to_vec(&serde_json::json!({
                    "spec":{
                        "template":{
                        "spec": {
                            "volumes":[
                                {
                                    "name":"config-volume",
                                    "configMap":{
                                    "name": swir_label,
                                    "items":[
                                        {
                                        "key":swir_label,
                                        "path":"config.yaml"
                                        }
                                    ]
                                    }
                                }
                            ]
                        }
                        }
                    }
                    }))
                    .unwrap();

                    match api.patch(&name, &patch_params, volumes_patch).await.context(SwirPatchingFailed) {
                        Ok(_res) => {
                            info!("Patched volumes {} {}", name, namespace);
                            match api.patch(&name, &patch_params, spec_patch).await.context(SwirPatchingFailed) {
                                Ok(_res) => {
                                    info!("Patched containers {} {}", name, namespace);
                                    reconciller_action
                                }
                                Err(err) => {
                                    warn!("{:?}", err);
                                    Err(err)
                                }
                            }
                        }
                        Err(err) => {
                            warn!("{:?}", err);
                            Err(err)
                        }
                    }
                } else {
                    warn!("Unable to create config map {} {}", namespace, swir_label);
                    reconciller_action
                }
            } else {
                warn!("No config for {} {}", namespace, swir_label);
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
    config_source: Box<dyn ConfigSource>,
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    std::env::set_var("RUST_LOG", "info,kube-runtime=info,kube=info");
    env_logger::init();
    let client = Client::try_default().await.unwrap();
    //    let config_source = DummyConfigSource();
    let config_source = FolderBasedConfigSource("./".to_string());
    debug! {"Running "};
    let cmgs = Api::<Deployment>::all(client.clone());
    let cms = Api::<Deployment>::all(client.clone());
    let lp1 = ListParams::default().labels("swir");
    let lp2 = ListParams::default().labels("swir");
    Controller::new(cmgs, lp1)
        .owns(cms, lp2)
        .run(
            reconcile,
            error_policy,
            Context::new(Data {
                client,
                config_source: Box::new(config_source),
            }),
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => info!("{:?}", o),
                Err(e) => warn!("reconcile failed: {}", e),
            }
        })
        .await;

    Ok(())
}
