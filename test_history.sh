#!/usr/bin/env bash

batch=0
RLTBL='rltbl -vv'

case $1 in
  "-help"|"--help"|-h)
    echo "Usage: `basename $0` [--batch]"
    exit 1
  ;;
  "--batch")
    batch=1
    shift
  ;;
esac

make sqlx

echo && echo "Proceeding to step 1"
test $batch -ne 1 && echo -n "Press enter " && read enter
${RLTBL} demo --size 20 --force || exit 1

rltbl -v get table penguin > penguin.tsv

echo '{"species": "FOO"}' | RLTBL_USER=mike rltbl -v --input JSON add row penguin
RLTBL_USER=ahmed rltbl -v move row penguin 13 11
RLTBL_USER=barbara rltbl -v move row penguin 7 10
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=afreen rltbl -v move row penguin 20 18
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=afreen rltbl -v set value penguin 16 species Sq8mktUFBW9Yz
RLTBL_USER=ahmed rltbl -v move row penguin 11 15
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v move row penguin 4 5
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v set value penguin 6 species 004pu2m57Ba1C
echo '{"species": "FOO"}' | RLTBL_USER=afreen rltbl -v --input JSON add row penguin
RLTBL_USER=mike rltbl -v move row penguin 1 4
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v set value penguin 1 species 46el5RVPoOaBX
echo '{"species": "FOO"}' | RLTBL_USER=afreen rltbl -v --input JSON add row penguin
RLTBL_USER=ahmed rltbl -v set value penguin 11 species VN4h7fFZ7AP2o
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v set value penguin 2 species Bd0a5KiBNnfs7
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v move row penguin 2 4
RLTBL_USER=afreen rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
echo '{"species": "FOO"}' | RLTBL_USER=ahmed rltbl -v --input JSON add row penguin
RLTBL_USER=mike rltbl -v move row penguin 5 3
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=barbara rltbl -v set value penguin 10 species Ei6461STNFfxe
RLTBL_USER=mike rltbl -v set value penguin 1 species 1ItuZLpTlHMdU
echo '{"species": "FOO"}' | RLTBL_USER=ahmed rltbl -v --input JSON add row penguin
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=afreen rltbl -v set value penguin 16 species bkNsTBIswDNfg
RLTBL_USER=mike rltbl -v move row penguin 1 4
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v set value penguin 11 species 8GXfiSWUdlIE4
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v move row penguin 13 14
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
echo '{"species": "FOO"}' | RLTBL_USER=ahmed rltbl -v --input JSON add row penguin
RLTBL_USER=afreen rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v set value penguin 9 species GwUq8K9NGtuSy
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v set value penguin 7 species GyVszn6OolHjk
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=barbara rltbl -v set value penguin 6 species rf306KuGwqv5Y
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=barbara rltbl -v set value penguin 9 species zLN8JVEZkTqkh
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=mike rltbl -v undo

rltbl -v get table penguin | diff - penguin.tsv

if [ $? -eq 0 ]
then
    echo && echo "All done!"
fi
