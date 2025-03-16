#/bin/bash

PATH="target/debug:$PATH"
RLTBL='rltbl -v'
RLTBL_DB=.relatable/relatable.db
SQLITE="sqlite3 -init <(echo .timeout 1000) $RLTBL_DB"

MIN_SLEEP=1
MAX_SLEEP=4
NUM_RETRIES=5

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
        echp "${command}"
        eval "${command}"
    done
    if [[ $more_tries -eq 0 ]]
    then
        echo "Giving up"
        exit 1
    fi
}

act_randomly () {
    user=$1
    min_row=$2
    max_row=$3

    for action in $(rltbl_test generate-seq --min-length 5 --max-length 10 penguin)
    do
        skip=$((0 + $RANDOM % 4))
        if [[ $skip -eq 0 ]]
        then
            echo "${user} is taking a break"
            sleep 1
        fi

        row=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
        case $action in
            # We treat "add" and "delete" as synonyms for update here, since adding and deleting
            # rows in a random test like this introduces complications that are not really
            # relevant to what we are trying to test here.
            "add")
                command="echo '{\"species\": \"FOO\"}' | RLTBL_USER=${user} ${RLTBL} --input JSON add row penguin"
                echo "${command}"
                retry_and_fail "${command}"
                ;;
            # We treat "delete" and "update" as synonyms for update here, since the precise
            # operation performed is not really what we are testing in this test, and deleting
            # rows introduces complications with multiple users that are not really relevant.
            "delete" | "update")
                value=$(tr -dc A-Za-z0-9 </dev/urandom | tail -n +1 | head -c 13)
                command="RLTBL_USER=${user} ${RLTBL} set value penguin ${row} species ${value}"
                echo "${command}"
                retry_and_fail "${command}"
                ;;
            "move")
                row_to_move=$row
                where_to_move_after=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
                while [[ ${where_to_move_after} == ${row_to_move} ]]
                do
                    where_to_move_after=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
                done
                command="RLTBL_USER=${user} ${RLTBL} move row penguin ${row_to_move} ${where_to_move_after}"
                echo "${command}"
                retry_and_fail "${command}"
                ;;
            "undo")
                command="RLTBL_USER=${user} ${RLTBL} undo"
                echo "${command}"
                retry_and_fail "${command}"
                ;;
            "redo")
                command="RLTBL_USER=${user} ${RLTBL} redo"
                echo "${command}"
                retry_and_fail "${command}"
                ;;
            *) echo "Unrecognized action ${action} for ${user}"
               exit 1
               ;;
        esac
        if [[ $? -ne 0 ]]
        then
            echo "${user} encountered an error and is giving up"
            exit 1
        fi
        sleep 1
    done
}


### Execution begins here

command="${RLTBL} demo --size 20 --force"
echo $command
eval "$command"

(
    act_randomly mike 1 5
    echo "mike is done"
) &

sleep 0.25

(
    act_randomly barbara 6 10
    echo "barbara is done"
) &

sleep 0.25

(
    act_randomly ahmed 11 15
    echo "ahmed is done"
) &

sleep 0.25

(
    act_randomly afreen 16 20
    echo "afreen is done"
) &

wait || exit 1

# Here is a scenario in which the test will fail. 1) mike moves row 15. 2) barbara moves row 16.
# 3) Mike undos. 4) Barbara undos. The problem is that the initial row that 16 comes after in
# step 2 is determined dynamically. So from barbara's point of view the row goes from being after
# row 14 to somewhere else, and then back again. It should have come back to being after 15, but
# is prevented by mike's actions. This scenario is uncommon, though, and it isn't clear how to
# guard against it.

rltbl get table penguin > /var/tmp/table.$$
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
if [[ $status -eq 0 ]]
then
    echo "Test successful"
else
    echo "Exiting with error"
fi

exit $status
