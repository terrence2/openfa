#!/usr/bin/env python3
import argparse
import json
import os
import sys
from multiprocessing.pool import ThreadPool
import zipfile

def main():
    parser = argparse.ArgumentParser(description='Download srtm tiles.')
    parser.add_argument('-i', '--index', metavar='FILE', required=True, help='the index of files to download')
    parser.add_argument('-d', '--directory', metavar='DIR', required=True, help='the directory to find input tiles in')
    parser.add_argument('-o', '--output', metavar='DIR', required=True, help='the directory to output tiles to')
    args = parser.parse_args()

    output_dir = args.output
    assert os.path.isdir(output_dir)

    index_file = args.index
    with open(index_file, 'r') as fp:
        data = json.loads(fp.read())
    assert data['type'] == 'FeatureCollection'

    check_list = []
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
        source = os.path.join(args.directory, filename)
        check_list.append((len(check_list) + 1, len(data['features']), source, args.output))

    print("checking {} tiles".format(len(check_list)))

    for item in check_list:
        check_file(item)

#     results = ThreadPool(4).imap_unordered(fetch_url, download_list)
#     for path in results:
#         print(f"finished {path}")

def check_file(entry):
    index, count, source_zip, output_dir = entry
    with zipfile.ZipFile(source_zip, 'r') as zip_ref:
        print(f"extracting: {index} of {count}: {source_zip}")
        zip_ref.extractall(output_dir)

if __name__ == '__main__':
    main()
