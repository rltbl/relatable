```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl serve --port 9000 --timeout 5 &
...
$ curl 'http://0.0.0.0:9000/table/penguin?island=eq.Biscoe'
...
```
