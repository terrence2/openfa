#!/bin/bash
set -e

if [ ! -e LICENSE ]
then
    echo "This script must be run in the root directory of the openfa project"
    exit 1
fi

#mkdir -p test_data/{un,}packed/{USNF,MF,ATF,ATFNF,ATFG,USNF97,FA}

for GAME in $(ls --color=never test_data/packed/);
do
    LIBS=`ls --color=never test_data/packed/$GAME/* | xargs -n1 realpath`
    mkdir -p test_data/unpacked/$GAME

    pushd libs/lib
    cargo run --release -- unpack -o ../../test_data/unpacked/$GAME $LIBS
    popd
done
