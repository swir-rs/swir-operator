#!/bin/bash
echo "**************************"
echo "Building SWIR operator image"
echo ""
echo "This is slow and takes time on the first build"
echo ""
echo "**************************"
docker rmi --no-prune swir/swir-operator:v0.3.2
docker build -t swir/swir-operator:v0.3.2 -f Dockerfile_local .    
echo "**************************"
