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

echo '{"species": "FOO"}' | RLTBL_USER=barbara rltbl -v --input JSON add row penguin
echo '{"species": "FOO"}' | RLTBL_USER=ahmed rltbl -v --input JSON add row penguin
RLTBL_USER=afreen rltbl -v set value penguin 18 species S2fEAjwGuCWRm
RLTBL_USER=mike rltbl -v set value penguin 3 species aZ4IGeQnOOGok
RLTBL_USER=barbara rltbl -v set value penguin 10 species FA1MFN1epKwCu
RLTBL_USER=ahmed rltbl -v move row penguin 11 14
RLTBL_USER=afreen rltbl -v set value penguin 20 species SYtOYZlJMau8N
RLTBL_USER=mike rltbl -v set value penguin 4 species QXOTCg3G2X5NM
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v set value penguin 13 species m95Y1pvfuMNMH
echo '{"species": "FOO"}' | RLTBL_USER=mike rltbl -v --input JSON add row penguin
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=afreen rltbl -v set value penguin 16 species DoE2aqsNLlv7J
RLTBL_USER=mike rltbl -v move row penguin 3 1
RLTBL_USER=barbara rltbl -v set value penguin 10 species d8BtEaXlI1Adp
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v undo
echo '{"species": "FOO"}' | RLTBL_USER=ahmed rltbl -v --input JSON add row penguin
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=afreen rltbl -v set value penguin 18 species PQlsFup94U7gd
RLTBL_USER=mike rltbl -v set value penguin 5 species tvN7d6JoEfBNZ
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v set value penguin 4 species pQGgbJfQ35knN
RLTBL_USER=barbara rltbl -v undo
echo '{"species": "FOO"}' | RLTBL_USER=afreen rltbl -v --input JSON add row penguin
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v set value penguin 14 species fRiIT4Mk1NCTq
RLTBL_USER=barbara rltbl -v set value penguin 8 species cYgScAn0C4jpj
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v move row penguin 6 9
RLTBL_USER=afreen rltbl -v undo
RLTBL_USER=mike rltbl -v set value penguin 3 species vmUk4UynZ3Inx
RLTBL_USER=ahmed rltbl -v move row penguin 13 15
RLTBL_USER=barbara rltbl -v move row penguin 7 10
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=mike rltbl -v set value penguin 2 species JZkMrt7k78eT3
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v move row penguin 15 12
RLTBL_USER=barbara rltbl -v move row penguin 10 9
RLTBL_USER=mike rltbl -v set value penguin 4 species QjnLqpFBabdKI
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v move row penguin 6 7
RLTBL_USER=mike rltbl -v set value penguin 4 species 6o5opu9nlVA54
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v set value penguin 13 species lHqPJfZzq8EY3
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v move row penguin 15 14
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v move row penguin 11 12
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=barbara rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v redo
RLTBL_USER=barbara rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=ahmed rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v undo
RLTBL_USER=mike rltbl -v redo
RLTBL_USER=mike rltbl -v undo

rltbl -v get table penguin | diff - penguin.tsv

if [ $? -eq 0 ]
then
    echo && echo "All done!"
fi
