#!/bin/bash
echo "**************************"
echo "Building SWIR operator image"
echo ""
echo "This is slow and takes time on the first build"
echo ""
echo "**************************"
#docker rmi --no-prune swir/swir:v0.4.0
#docker build -t swir/swir:v0.4.0 -f Dockerfile_local .

default_version=v0.4.0

if [ -z "$2" ]
then
    version=$default_version    
else
    version=$2

fi

docker rmi --no-prune swir/swir-operator:$version
docker build -f build/Dockerfile_build_stage1 -t swir_operator_builder:latest .
if [ -z "$1" ]
then
    docker build -t swir/swir-operator:$version -f build/Dockerfile_build_stage2 .
else
    docker build -t swir/swir-operator:$version -f build/Dockerfile_build_stage2 .    
fi
echo "**************************"



