#!/bin/bash
set -e

if [ ! -e LICENSE ]
then
    echo "This script must be run in the root directory of the openfa project"
    exit 1
fi

if [ $# -eq 0 ]
then
    echo "No arguments supplied"
    echo "Expected the directory containing unpacked libs"
    exit 1
fi

SH_FILES=`find "$1" -name "*.SH"`
PAL_FILES=`find "$1" -name "*.PAL"`
PIC_FILES=`find "$1" -name "*.PIC"`
ENT_FILES=`find "$1" -name "*.[JNOP]T"`

mkdir -p libs/sh/test_data
mkdir -p libs/pal/test_data
mkdir -p libs/pic/test_data
mkdir -p libs/peff/test_data
mkdir -p libs/entity/test_data

cp -v $SH_FILES libs/sh/test_data/
cp -v $PAL_FILES libs/pal/test_data/
cp -v $PIC_FILES libs/pic/test_data/
cp -v $ENT_FILES libs/entity/test_data/
