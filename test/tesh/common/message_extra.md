```console tesh-session="test"
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force
Created a demonstration database in '...'
$ rltbl set value island 3 island Montreal
$ rltbl get table message
...
Rows 1-2 of 2
message_id  added_by  table    row  column  value  level  rule         message
1           rltbl     penguin  8    island  Dream  error  key:foreign  island must be in island.island
2           rltbl     penguin  9    island  Dream  error  key:foreign  island must be in island.island
$ rltbl set value island 3 island Dream
$ rltbl get table message
...
Rows 1-0 of 0
message_id  added_by  table  row  column  value  level  rule  message
$ rltbl set value island 3 island Montreal
$ rltbl save
$ rltbl drop database
$ rltbl demo --size 0 --force
Created a demonstration database in '...'
$ rltbl load table island.tsv --force
$ rltbl load table penguin.tsv --force
$ rltbl get table message
...
Rows 1-2 of 2
message_id  added_by  table    row  column  value  level  rule         message
1           rltbl     penguin  8    island  Dream  error  key:foreign  island must be in island.island
2           rltbl     penguin  9    island  Dream  error  key:foreign  island must be in island.island
```
