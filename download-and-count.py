#!/usr/bin/env python3

import sys, subprocess, tarfile, os, tempfile, multiprocessing
import urllib.request, urllib.error

def run(crate, ver):
    res = ''
    cmd = 'count-unsafe'
    link = 'https://crates.io/api/v1/crates/%s/%s/download' % (crate, ver)

    with tempfile.TemporaryDirectory() as tmp_dir:
        path = os.path.join(tmp_dir, 'a.tar')
        try:
            urllib.request.urlretrieve(link, filename=path)
            tf = tarfile.open(path)
            tf.extractall(tmp_dir)
            crate_dir = os.path.join(tmp_dir, '%s-%s' % (crate, ver), 'src')
            proc = subprocess.run([cmd, crate_dir], capture_output=True)
            res = proc.stdout.decode('utf-8')

        except urllib.error.HTTPError:
            pass

    return (crate, ver, res)


def main(argv):
    if len(argv) != 2:
        print("Usage: ./download-and-count.py [file]")
        return

    f = open(argv[1], 'r')
    
    l = f.readlines()
    l = [x.strip() for x in l]
    l = [x.split(' ') for x in l if x.startswith('Compiling')]
    l = [(x[1], x[2][1:]) for x in l]
    
    with multiprocessing.Pool(8) as p:
        res = p.starmap(run, l)

    for (c, v, l) in res:
        print('%s v%s' % (c, v))
        print(l)
    

if __name__ == "__main__":
    main(sys.argv)
