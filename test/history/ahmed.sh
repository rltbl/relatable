#!/bin/bash

export RLTBL_USER=ahmed
sleep_max=3

rltbl delete row penguin 6 2>/dev/null
sleep $((0 + $RANDOM % $sleep_max))
while [ $? -ne 0 ]
do
    rltbl delete row penguin 6 2>/dev/null
    sleep $((0 + $RANDOM % $sleep_max))
done

rltbl delete row penguin 5 2>/dev/null
sleep $((0 + $RANDOM % $sleep_max))
while [ $? -ne 0 ]
do
    rltbl delete row penguin 5 2>/dev/null
    sleep $((0 + $RANDOM % $sleep_max))
done

rltbl set value penguin 4 species Cow 2>/dev/null
sleep $((0 + $RANDOM % $sleep_max))
while [ $? -ne 0 ]
do
    rltbl set value penguin 4 species Cow 2>/dev/null
    sleep $((0 + $RANDOM % $sleep_max))
done

rltbl undo 2>/dev/null
sleep $((0 + $RANDOM % $sleep_max))
while [ $? -ne 0 ]
do
    rltbl undo 2>/dev/null
    sleep $((0 + $RANDOM % $sleep_max))
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl history | sed "s/\x1B\[\([0-9]\{1,2\}\(;[0-9]\{1,2\}\)\?\)\?[mGK]//g" > /var/tmp/history.$$

diff /var/tmp/history.$$ - <<EOF
  Delete row 6
▲ Delete row 5
▼ Update 'species' in row 4 from Cow to Pygoscelis adeliae
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/history.$$
exit $status

