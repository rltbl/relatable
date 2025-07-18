PATH="target/debug:$PATH"
RLTBL='rltbl -v'

STEP_SLEEP=1
MIN_RETRY_SLEEP=1
MAX_RETRY_SLEEP=4
NUM_RETRIES=5

varying_rate=0
case $1 in
    "--varying-rate")
        varying_rate=1
        shift
        ;;
    "")
        shift
        ;;
    "-help"|"--help"|-h)
        echo "Usage: `basename $0` [ --varying-rate ]"
        exit 0
        ;;
    *)
        echo "Usage: `basename $0` [ --varying-rate ]"
        exit 1
        ;;
esac

custom_sleep () {
    if [[ ${varying_rate} -eq 0 ]]
    then
        sleep $STEP_SLEEP
    else
        sleep_val=$(printf '%s\n' $(echo "scale=10; $RANDOM/32768 * 2" | bc ))
        sleep ${sleep_val}
    fi
}


retry_and_fail () {
    command=$1
    user=$2

    more_tries=${NUM_RETRIES}
    eval "${command}" 2>&1
    while [[ $? -ne 0 && $more_tries -gt 0 ]]
    do
        sleep_val=$(($MIN_RETRY_SLEEP + $RANDOM % $MAX_RETRY_SLEEP))
        echo "${user} will retry in ${sleep_val}s ..."
        sleep ${sleep_val}
        more_tries=`expr ${more_tries} - 1`
        echo "${command}"
        eval "${command}" 2>&1
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
            custom_sleep
        fi

        row=$(($min_row + $RANDOM % $(expr $max_row - $min_row + 1)))
        case $action in
            "add")
                command="echo '{\"species\": \"FOO\"}' | RLTBL_USER=${user} ${RLTBL} --input JSON add row penguin"
                echo "${command}"
                retry_and_fail "${command}" ${user}
                ;;
            # We treat "delete" and "update" as synonyms for update here, since the precise
            # operation performed is not really what we are testing in this test, and deleting
            # rows introduces complications with multiple users that are not really relevant.
            "delete" | "update")
                value=$(tr -dc A-Za-z0-9 </dev/urandom | tail -n +1 | head -c 13)
                command="RLTBL_USER=${user} ${RLTBL} set value penguin ${row} species ${value}"
                echo "${command}"
                retry_and_fail "${command}" ${user}
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
                retry_and_fail "${command}" ${user}
                ;;
            "undo")
                command="RLTBL_USER=${user} ${RLTBL} undo"
                echo "${command}"
                retry_and_fail "${command}" ${user}
                ;;
            "redo")
                command="RLTBL_USER=${user} ${RLTBL} redo"
                echo "${command}"
                retry_and_fail "${command}" ${user}
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
        custom_sleep
    done
}


### Execution begins here

echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt

command="${RLTBL} demo --size 20 --force"
echo $command
output=$(eval "$command" | diff - expected_output.txt)
cat expected_output.txt

if [[ $output != "" ]]
then
    echo "Unexpected output"
    exit 1
fi

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

# Here is a scenario in which this test will fail. 1) mike moves row 15. 2) barbara moves row 16.
# 3) Mike undos. 4) Barbara undos. The problem is that the initial row that 16 comes after in
# step 2 is determined dynamically. So from barbara's point of view, the row she moves (16) goes
# from being after 14 (because mike has moved 15) to somewhere else, and then back to after 14
# again. From the global perspective, it should have come back to being after 15, but mike's
# action erases the information about row 16's initially coming after 15. Given the current design
# there does not appear to be anything that we can do about this, since we do not store the initial
# "after row" in the database. We could change the design. However the scenario appears to be
# uncommon. When this test is run repeatedly, it passes roughly 9 times out of 10.

rltbl get table penguin > /var/tmp/table.$$
diff /var/tmp/table.$$ - <<EOF
Rows 1-20 of 20
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1A1           44.6         31.1        4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N1A2           30.5         33.4        3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N2A1           35.2         22.4        4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N2A2           34.3         35.8        3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N3A1           40.6         39.9        2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N3A2           30.9         22.2        4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N4A1           38.6         28.5        3607
FAKE123     8              Pygoscelis adeliae  Dream      N4A2           33.8         39.9        1908
FAKE123     9              Pygoscelis adeliae  Dream      N5A1           43.7         23.1        3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           31.5         30.0        4521
FAKE123     11             Pygoscelis adeliae  Torgersen  N6A1           39.5         37.5        4174
FAKE123     12             Pygoscelis adeliae  Torgersen  N6A2           44.6         21.2        4700
FAKE123     13             Pygoscelis adeliae  Biscoe     N7A1           34.3         28.7        4908
FAKE123     14             Pygoscelis adeliae  Dream      N7A2           43.5         20.3        4274
FAKE123     15             Pygoscelis adeliae  Biscoe     N8A1           47.1         32.3        3803
FAKE123     16             Pygoscelis adeliae  Torgersen  N8A2           45.7         33.3        4458
FAKE123     17             Pygoscelis adeliae  Biscoe     N9A1           46.3         30.3        4444
FAKE123     18             Pygoscelis adeliae  Torgersen  N9A2           47.3         23.3        1350
FAKE123     19             Pygoscelis adeliae  Biscoe     N10A1          37.0         37.9        1749
FAKE123     20             Pygoscelis adeliae  Torgersen  N10A2          40.4         32.4        4906

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
