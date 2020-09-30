[![Build Status](https://travis-ci.com/swir-rs/swir-operator.svg?branch=master)](https://travis-ci.com/swir-rs/swir-operator)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![GitHub release](https://img.shields.io/github/release/swir-rs/swir-operator.svg)](https://GitHub.com/Naereen/StrapDown.js/releases/)
[![Awesome Badges](https://img.shields.io/badge/badges-awesome-green.svg)](https://swir.rs)


# swir-operator 
## SWIR Operator for Kubernetes

![](graphics/swir_logo_operator.png)

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
kubectl create ns swir
kubectl -n swir apply -f demo_deployment 
kubectl -n swir describe deployment swir-demo
```
  
# Requirements
- Rust 1.44.1 or above



	
