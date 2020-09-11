#!/bin/bash
echo "**************************"
echo "Building SWIR operator image"
echo ""
echo "This is slow and takes time on the first build"
echo ""
echo "**************************"
docker rmi --no-prune swir/swir-operator:v3
docker build -t swir/swir-operator:v3 -f Dockerfile .    
echo "**************************"
