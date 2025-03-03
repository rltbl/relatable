#/bin/sh

rltbl demo --size 10 --force
test/history/mike.sh &
test/history/barbara.sh &
test/history/ahmed.sh &
test/history/afreen.sh &

wait

rltbl get table penguin > /var/tmp/table.$$
diff /var/tmp/table.$$ - <<EOF
Rows 1-11 of 11
study_name  sample_number  species             island     individual_id  culmen_length  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.6           3221
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.2           1491
FAKE123     2              Mink                Torgersen  N2             30.5           3685
FAKE123     4              Pygoscelis adeliae  Torgersen  N4             31.4           1874
FAKE123     7              Lion                Montreal   N7             49.9           2129
FAKE123     8              Tiger               Biscoe     N8             30.9           1451
FAKE123     9              Pygoscelis adeliae  Biscoe     N9             38.6           2702
FAKE123     10             Pygoscelis adeliae  Dream      N10            33.8           4697
null        null           FOO                 null       null           null           null
null        null           BAR                 null       null           null           null
null        null           FEW                 null       null           null           null
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/table.$$
exit $status
