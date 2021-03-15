#!/bin/bash
old_version=$1
new_version=$2

if [ ! -z "$1" ] && [ ! -z "$2" ]
then
    echo "Version $1 -> $2"
    find . -name "*.sh" -exec grep -Hn ":$old_version" '{}' \; -exec sed -i "s/:$old_version/:$new_version/g" {} \;
    find . -name "*.yaml" -exec grep -Hn ":$old_version" '{}' \; -exec sed -i "s/:$old_version/:$new_version/g" {} \;
else
    echo "$1"
    echo "$2"    
    echo "Straigt build, no version change"    
fi

./build.sh

