```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ rltbl -v demo --force --size 10 | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl_test -v select-test penguin individual_id egg N1
Select test successful
```
