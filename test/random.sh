#/bin/bash

PATH="target/debug:$PATH"
RLTBL='rltbl -v'
RLTBL_DB=.relatable/relatable.db
SQLITE="sqlite3 $RLTBL_DB"

MIN_SLEEP=0
MAX_SLEEP=2
NUM_RETRIES=10

retry_and_fail () {
    command=$1

    more_tries=${NUM_RETRIES}
    eval "${command}"
    while [[ $? -ne 0 && $more_tries -gt 0 ]]
    do
        sleep_val=$(($MIN_SLEEP + $RANDOM % $MAX_SLEEP))
        echo "Retrying in ${sleep_val}s ..."
        sleep ${sleep_val}
        more_tries=`expr ${more_tries} - 1`
        eval "${command}"
    done
    if [[ $more_tries -eq 0 ]]
    then
        echo "**************************** Giving up ****************************"
        exit 1
    fi
}

act_randomly () {
    min_row=$1
    max_row=$2

    for action in $(rltbl_test generate-seq penguin)
    do
        row=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
        case $action in
            # We treat "add" and "delete" as synonyms for update here, since adding and deleting
            # rows in a random test like this introduces complications that are not really
            # relevant to what we are trying to test here.
            "add" | "delete" | "update")
                echo "User ${RLTBL_USER}: Updating value of row ${row} to ${value}"
                value=$(tr -dc A-Za-z0-9 </dev/urandom | tail -n +1 | head -c 13)
                retry_and_fail '${RLTBL} set value penguin ${row} species ${value}'
                ;;
            "move")
                row_to_move=$row
                more_tries=${NUM_RETRIES}
                where_to_move_after=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
                echo "User ${RLTBL_USER}: Moving ${row_to_move} to after row ${where_to_move_after}"
                ${RLTBL} move row penguin ${row_to_move} ${where_to_move_after}
                while [[ $? -ne 0 && $more_tries -gt 0 ]]
                do
                    where_to_move_after=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
                    sleep_val=$(($MIN_SLEEP + $RANDOM % $MAX_SLEEP))
                    echo "Retrying in ${sleep_val}s ..."
                    sleep ${sleep_val}
                    more_tries=`expr ${more_tries} - 1`
                    echo "User ${RLTBL_USER}: This time moving to after row ${where_to_move_after}"
                    ${RLTBL} move row penguin ${row_to_move} ${where_to_move_after}
                done
                if [[ $more_tries -eq 0 ]]
                then
                    echo "**************************** Giving up ****************************"
                    exit 1
                fi
                ;;
            "undo")
                echo "User ${RLTBL_USER}: Undoing"
                retry_and_fail '${RLTBL} undo'
                ;;
            "redo")
                echo "User ${RLTBL_USER}: Redoing"
                retry_and_fail '${RLTBL} redo'
                ;;
            *) echo "Unrecognized action ${action}"
               exit 1
               ;;
        esac
        sleep $(($MIN_SLEEP + $RANDOM % $MAX_SLEEP))
    done
}


### Execution begins here

${RLTBL} demo --size 20 --force

(
    export RLTBL_USER=mike;
    act_randomly 1 5
    echo "$RLTBL_USER is done"
) &

(
    export RLTBL_USER=barbara;
    act_randomly 6 10
    echo "$RLTBL_USER is done"
) &

(
    export RLTBL_USER=ahmed;
    act_randomly 11 15
    echo "$RLTBL_USER is done"
) &

(
   export RLTBL_USER=afreen;
   act_randomly 16 20
   echo "$RLTBL_USER is done"
) &

wait


rltbl get table penguin | tee /var/tmp/table.$$
diff /var/tmp/table.$$ - <<EOF
Rows 1-20 of 20
study_name  sample_number  species             island     individual_id  culmen_length  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.6           3221
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.5           3685
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.2           1491
FAKE123     4              Pygoscelis adeliae  Torgersen  N4             31.4           1874
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             45.8           3469
FAKE123     6              Pygoscelis adeliae  Torgersen  N6             40.6           4875
FAKE123     7              Pygoscelis adeliae  Torgersen  N7             49.9           2129
FAKE123     8              Pygoscelis adeliae  Biscoe     N8             30.9           1451
FAKE123     9              Pygoscelis adeliae  Biscoe     N9             38.6           2702
FAKE123     10             Pygoscelis adeliae  Dream      N10            33.8           4697
FAKE123     11             Pygoscelis adeliae  Biscoe     N11            37.9           3237
FAKE123     12             Pygoscelis adeliae  Torgersen  N12            33.1           3883
FAKE123     13             Pygoscelis adeliae  Torgersen  N13            31.5           3012
FAKE123     14             Pygoscelis adeliae  Torgersen  N14            42.7           3989
FAKE123     15             Pygoscelis adeliae  Dream      N15            47.5           4174
FAKE123     16             Pygoscelis adeliae  Torgersen  N16            44.6           1252
FAKE123     17             Pygoscelis adeliae  Biscoe     N17            34.3           2747
FAKE123     18             Pygoscelis adeliae  Dream      N18            43.5           2516
FAKE123     19             Pygoscelis adeliae  Biscoe     N19            46.3           1276
FAKE123     20             Pygoscelis adeliae  Torgersen  N20            42.3           3803
EOF

status=$?
rm -f /var/tmp/table.$$ /var/tmp/table.$$
exit $status
