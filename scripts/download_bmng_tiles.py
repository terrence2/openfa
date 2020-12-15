#!/usr/bin/env python3
import argparse
import json
import os
import requests
import sys
from multiprocessing.pool import ThreadPool
from pathlib import Path
from requests.auth import HTTPBasicAuth

BASE_PATH = 'https://eoimages.gsfc.nasa.gov/images/imagerecords'
FILENAME = 'world.2004{:02}.3x21600x21600.{}{}.png'
MONTH_BASE_PATHS = {
    1: '/73000/73938/',
    2: '/73000/73967/',
    3: '/73000/73992/',
    4: '/74000/74017/',
    5: '/74000/74042/',
    6: '/76000/76487/',
    7: '/74000/74092/',
    8: '/74000/74117/',
    9: '/74000/74142/',
    10: '/74000/74167/',
    11: '/74000/74192/',
    12: '/74000/74218/',
}

def main():
    parser = argparse.ArgumentParser(description='Download bmng tiles.')
    parser.add_argument('-o', '--output', type=Path, metavar='DIR', required=True, help='the base directory to output tiles to')
    args = parser.parse_args()

    output_dir = args.output
    assert os.path.isdir(output_dir)

    index = 0
    for month, directory in MONTH_BASE_PATHS.items():
        os.makedirs(output_dir / f'month{month:02}', exist_ok=True)

        for lon in 'ABCD':
            for lat in '12':
                terminal = FILENAME.format(month, lon, lat)
                filename = output_dir / f'month{month:02}' / terminal
                url = BASE_PATH + directory + terminal
                fetch_url((index, filename, url))
                index += 1

def fetch_url(entry):
    index, path, uri = entry
    if os.path.exists(path):
        print(f"{index}: skipping {path}")
        return path

    print(f"{index}: fetching {uri}")
    r = requests.get(uri, stream=True)
    assert r.status_code == 200, f"failed to download: {uri}"
    with open(path, 'wb') as f:
        for chunk in r:
            f.write(chunk)

    return path


if __name__ == '__main__':
    main()
