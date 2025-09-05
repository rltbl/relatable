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

echo && echo "Proceeding to step 2"
test $batch -ne 1 && echo -n "Press enter " && read enter
echo '{"species": "FOO"}' | RLTBL_USER=mike ${RLTBL} --input JSON add row penguin || exit 1

echo && echo "Proceeding to step 3"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=mike ${RLTBL} undo || exit 1

echo && echo "Proceeding to step 4"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=afreen ${RLTBL} move row penguin 17 20 || exit 1

echo && echo "Proceeding to step 5"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=afreen ${RLTBL} undo || exit 1

# When 22 gets added, it seems like we need to update undone_afters for rows 17 and 21
# from [20] from [22], right?
echo && echo "Proceeding to step 6"
test $batch -ne 1 && echo -n "Press enter " && read enter
echo '{"species": "FOO"}' | RLTBL_USER=mike ${RLTBL} --input JSON add row penguin || exit 1

echo && echo "Proceeding to step 7"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=mike ${RLTBL} undo || exit 1

echo && echo "Proceeding to step 8"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=afreen ${RLTBL} move row penguin 20 18 || exit 1

echo && echo "Proceeding to step 9"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=mike ${RLTBL} redo || exit 1

echo && echo "Proceeding to step 10"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=afreen ${RLTBL} undo || exit 1

echo && echo "Proceeding to step 12"
test $batch -ne 1 && echo -n "Press enter " && read enter
RLTBL_USER=mike ${RLTBL} undo || exit 1



if [ $? -eq 0 ]
then
    echo && echo "All done!"
fi
