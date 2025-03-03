#!/bin/bash

# TODO: Add these to a tesh test case later.

rltbl='rltbl -v'

ask_what_to_do () {
    echo 'Press enter to continue or Ctrl-C to exit'
    read enter
}

exit_with_error () {
    exit 1
}

fail_action='ask_what_to_do'
# fail_action='exit_with_error'

echo ${fail_action}

{
    echo "----- Running TC 1 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "BAR"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "KEW"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}
    
    echo "----- Running TC 2 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} move row penguin 9 7 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} set value penguin 4 island Enderby || ${fail_action}
    ${rltbl} delete row penguin 9 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 3 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    ${rltbl} set value penguin 4 island Enderby || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} delete row penguin 9 || ${fail_action}
    ${rltbl} set value penguin 3 species Godzilla || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} move row penguin 3 5 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 4 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    ${rltbl} delete row penguin 5 || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} delete row penguin 10 || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action}

    ${rltbl} move row penguin 9 7 || ${fail_action}
    ${rltbl} move row penguin 4 8 || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 5 -----"
    make && ${rltbl} demo --size 30 --force || ${fail_action}
    ${rltbl} delete row penguin 1 || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} delete row penguin 3 || ${fail_action}
    ${rltbl} delete row penguin 7 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action} # -- 7
    ${rltbl} redo || ${fail_action} # -- 3
    
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 6 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} delete row penguin 6 || ${fail_action}
    ${rltbl} set value penguin 4 island Enderby || ${fail_action}
    ${rltbl} move row penguin 1 8 || ${fail_action}
    ${rltbl} undo || ${fail_action} # Undo move row
    ${rltbl} undo || ${fail_action} # Undo set value
    ${rltbl} undo || ${fail_action} # Undo delete row
    ${rltbl} undo || ${fail_action} # Undo add row
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 7 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "BAR"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "KEW"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 8 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "BAR"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    echo '{"species": "KEW"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} move row penguin 12 1 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 9 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}
    echo '{"species": "FOO"}' | ${rltbl} --input JSON add row penguin || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} move row penguin 4 9 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}
    ${rltbl} move row penguin 3 1 || ${fail_action}
    ${rltbl} move row penguin 4 2 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

    echo "----- Running TC 10 -----"
    make && ${rltbl} demo --size 10 --force || ${fail_action}

    ${rltbl} delete row penguin 6 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} delete row penguin 9 || ${fail_action}
    ${rltbl} undo || ${fail_action}
    ${rltbl} redo || ${fail_action}

    ${rltbl} undo || ${fail_action}
    ${rltbl} undo || ${fail_action}

    ${rltbl} get table penguin || ${fail_action}
    ${rltbl} history || ${fail_action}

} 2>&1
