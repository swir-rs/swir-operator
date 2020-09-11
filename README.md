[![Build Status](https://travis-ci.org/swir-rs/swir-operator.svg?branch=master)](https://travis-ci.org/swir-rs/swir-operator)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![GitHub release](https://img.shields.io/github/release/swir-rs/swir-operator.svg)](https://GitHub.com/Naereen/StrapDown.js/releases/)
[![Awesome Badges](https://img.shields.io/badge/badges-awesome-green.svg)](https://swir.rs)
# swir-operator
SWIR Operator for Kubernetes

![Logo](https://raw.githubusercontent.com/swir-rs/swir/master/graphics/swir_logo.png)


# Description
SWIR Operator is Kubernetes operator to inject and configure SWIR sidecars into your solution.

# Running
```
minikube start
```

In a terminal 


```
cargo run
```

In a different terminal


```
kubectl create ns swir-ns
kubectl -n swir-ns apply -f demo_deployment 
kubectl -n swir-ns describe swir-demo
```
  
# Requirements
- Rust 1.44.1 or above



