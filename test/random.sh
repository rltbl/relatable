#/bin/sh

PATH="target/debug:$PATH"
RLTBL='rltbl -v'
MAX_SLEEP=3

next_sample=11

inc_sample () {
    next_sample=`expr ${next_sample} + 1`
}

act_randomly () {
    for action in $(rltbl_test generate-seq penguin)
    do
        case $action in
            "add")
                echo "User ${RLTBL_USER}: Adding row for sample_number ${next_sample}"
                echo "{\"sample_number\": ${next_sample}}" | ${RLTBL} --input JSON add row penguin
                inc_sample
                ;;
            "delete")
                row=$((1 + $RANDOM % next_sample))
                echo "User ${RLTBL_USER}: Deleting row ${row}"
                ${RLTBL} delete row penguin ${row}
                ;;
            "update")
                row=$((1 + $RANDOM % next_sample))
                value=$(tr -dc A-Za-z0-9 </dev/urandom | head -c 13)
                echo "User ${RLTBL_USER}: Updating species in row ${row} to ${value}"
                ${RLTBL} set value penguin ${row} species ${value}
                ;;
            "move")
                row_to_move=$((1 + $RANDOM % next_sample))
                where_to_move_after=$((1 + $RANDOM % next_sample))
                while [[ ${where_to_move_after} == ${row_to_move} ]]
                do
                    where_to_move_after=$((1 + $RANDOM % next_sample))
                done
                echo "User ${RLTBL_USER}: Moving ${row_to_move} to after row ${where_to_move_after}"
                ${RLTBL} move row penguin ${row_to_move} ${where_to_move_after}
                ;;
            "undo")
                echo "User ${RLTBL_USER}: Undoing"
                ${RLTBL} undo
                ;;
            "redo")
                echo "User ${RLTBL_USER}: Redoing"
                ${RLTBL} redo
                ;;
            *) echo "Unrecognized action ${action}"
               exit 1
               ;;
        esac
        sleep $((0 + $RANDOM % $MAX_SLEEP))
    done
}


(
    export RLTBL_USER=mike;
    act_randomly
    echo "$RLTBL_USER is done"
) # &
# 
# (
#     export RLTBL_USER=barbara;
#     act_randomly
#     echo "$RLTBL_USER is done"
# ) &
# 
# (
#     export RLTBL_USER=ahmed;
#     act_randomly
#     echo "$RLTBL_USER is done"
# ) &
# 
# (
#     export RLTBL_USER=afreen;
#     act_randomly
#     echo "$RLTBL_USER is done"
# ) &
# 
# wait


# rltbl get table penguin > /var/tmp/table.$$
# diff /var/tmp/table.$$ - <<EOF
# Rows 1-11 of 11
# study_name  sample_number  species             island     individual_id  culmen_length  body_mass
# FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.6           3221
# FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.2           1491
# FAKE123     2              Mink                Torgersen  N2             30.5           3685
# FAKE123     4              Pygoscelis adeliae  Torgersen  N4             31.4           1874
# FAKE123     7              Lion                Montreal   N7             49.9           2129
# FAKE123     8              Tiger               Biscoe     N8             30.9           1451
# FAKE123     9              Pygoscelis adeliae  Biscoe     N9             38.6           2702
# FAKE123     10             Pygoscelis adeliae  Dream      N10            33.8           4697
# null        null           FOO                 null       null           null           null
# null        null           BAR                 null       null           null           null
# null        null           FEW                 null       null           null           null
# EOF

# status=$?
# rm -f /var/tmp/table.$$ /var/tmp/table.$$
# exit $status
