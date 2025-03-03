#!/bin/bash

export RLTBL_USER=barbara
sleep_max=3

sleep $((0 + $RANDOM % $sleep_max))
rltbl move row penguin 2 3 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl move row penguin 2 3 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl set value penguin 2 species Mink 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl set value penguin 2 species Mink 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl move row penguin 1 2 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl move row penguin 1 2 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl undo 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl undo 2>/dev/null
done


sleep $((0 + $RANDOM % $sleep_max))
rltbl history | sed "s/\x1B\[\([0-9]\{1,2\}\(;[0-9]\{1,2\}\)\?\)\?[mGK]//g" > /var/tmp/history.$$

diff /var/tmp/history.$$ - <<EOF
  Move row 2 from after row 1 to after row 3
▲ Update 'species' in row 2 from Pygoscelis adeliae to Mink
▼ Move row 1 from after row 2 to after row 0
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/history.$$
exit $status
