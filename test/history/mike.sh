#!/bin/bash

export RLTBL_USER=mike
sleep_max=3

sleep $((0 + $RANDOM % $sleep_max))
echo '{"species": "FOO"}' | rltbl --input JSON add row penguin 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    echo '{"species": "FOO"}' | rltbl --input JSON add row penguin 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
echo '{"species": "BAR"}' | rltbl --input JSON add row penguin 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    echo '{"species": "BAR"}' | rltbl --input JSON add row penguin 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
echo '{"species": "KEW"}' | rltbl --input JSON add row penguin 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    echo '{"species": "KEW"}' | rltbl --input JSON add row penguin 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl undo
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max)) 2>/dev/null
    rltbl undo
done

sleep $((0 + $RANDOM % $sleep_max))
echo '{"species": "QEW"}' | rltbl --input JSON add row penguin 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    echo '{"species": "QEW"}' | rltbl --input JSON add row penguin 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl undo 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl undo 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl redo 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl redo 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl set value penguin 13 species FEW 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl set value penguin 13 species FEW 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl history | sed "s/\x1B\[\([0-9]\{1,2\}\(;[0-9]\{1,2\}\)\?\)\?[mGK]//g" > /var/tmp/history.$$

diff /var/tmp/history.$$ - <<EOF
  Add row 13 after row 12
▲ Update 'species' in row 13 from QEW to FEW
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/history.$$
exit $status
