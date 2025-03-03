#!/bin/bash

export RLTBL_USER=afreen
sleep_max=3

sleep $((0 + $RANDOM % $sleep_max))
rltbl move row penguin 8 7 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl move row penguin 8 7 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl set value penguin 7 island Montreal 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl set value penguin island Montreal 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl set value penguin 7 species Lion 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl set value penguin 7 species Lion 2>/dev/null
done

sleep $((0 + $RANDOM % $sleep_max))
rltbl set value penguin 8 species Tiger 2>/dev/null
while [ $? -ne 0 ]
do
    sleep $((0 + $RANDOM % $sleep_max))
    rltbl set value penguin 8 species Tiger 2>/dev/null
done


sleep $((0 + $RANDOM % $sleep_max))
rltbl history | sed "s/\x1B\[\([0-9]\{1,2\}\(;[0-9]\{1,2\}\)\?\)\?[mGK]//g" > /var/tmp/history.$$

diff /var/tmp/history.$$ - <<EOF
  Move row 8 from after row 7 to after row 7
  Update 'island' in row 7 from Torgersen to Montreal
  Update 'species' in row 7 from Pygoscelis adeliae to Lion
▲ Update 'species' in row 8 from Pygoscelis adeliae to Tiger
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/history.$$
exit $status

