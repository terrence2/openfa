#!/usr/bin/env python3
import argparse
import json
import os
import requests
import sys
from multiprocessing.pool import ThreadPool
from requests.auth import HTTPBasicAuth

BASE_PATH = 'https://e4ftl01.cr.usgs.gov/MEASURES/SRTMGL1.003/2000.02.11/'

def main():
    parser = argparse.ArgumentParser(description='Download srtm tiles.')
    parser.add_argument('-i', '--index', metavar='FILE', required=True, help='the index of files to download')
    parser.add_argument('-o', '--output', metavar='DIR', required=True, help='the directory to output tiles to')
    args = parser.parse_args()

    output_dir = args.output
    assert os.path.isdir(output_dir)

    index_file = args.index
    with open(index_file, 'r') as fp:
        data = json.loads(fp.read())
    assert data['type'] == 'FeatureCollection'

    download_list = []
    for feature in data['features']:
        # {'type': 'Feature',
        #  'geometry': {
        #     'type': 'Polygon',
        #     'coordinates': [[[5.99972222, -0.00027778], [7.00027778, -0.00027778], [7.00027778, 1.00027778], [5.99972222, 1.00027778], [5.99972222, -0.00027778]]]},
        #     'properties': {'dataFile': 'N00E006.SRTMGL1.hgt.zip'}}
        assert feature['type'] == 'Feature'
        assert feature['geometry']['type'] == 'Polygon'
        assert len(feature['geometry']['coordinates']) == 1
        coord = feature['geometry']['coordinates'][0]
        south_latitude = coord[0][0]
        west_longitude = coord[0][1]

        filename = feature['properties']['dataFile']
        source = BASE_PATH + filename
        target = os.path.join(output_dir, filename)
        download_list.append((len(download_list) + 1, target, source))

    print("downloading {} tiles".format(len(download_list)))

    #fetch_url(download_list[0])

    results = ThreadPool(4).imap_unordered(fetch_url, download_list)
    for path in results:
        print(f"finished {path}")

def fetch_url(entry):
    index, path, uri = entry
    cookies = {'DATA': os.environ['COOKIE']}
    if os.path.exists(path):
        print(f"{index}: skipping {path}")
        return path

    print(f"{index}: fetching {uri}")
    r = requests.get(uri, stream=True, cookies=cookies)
    assert r.status_code == 200, f"failed to download: {uri}"
    with open(path, 'wb') as f:
        for chunk in r:
            f.write(chunk)

    return path


if __name__ == '__main__':
    main()
