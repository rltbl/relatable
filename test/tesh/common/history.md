```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force | diff - expected_output.txt
$ rm -f expected_output.txt

$ rltbl get table penguin > penguin.tsv

$ echo '{"species": "FOO", "sample_number": 25}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl delete row penguin 6
$ rltbl set value penguin 4 sample_number 26
$ rltbl move row penguin 1 8
$ rltbl undo # Undo move row
$ rltbl undo # Undo set value
$ rltbl undo # Undo delete row
$ rltbl undo # Undo add row

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'sample_number' in row 4 from 26 to 4 (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 12 1
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl move row penguin 4 9
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 3 1
$ rltbl move row penguin 4 2
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo

$ rltbl undo
$ rltbl undo
$ rltbl redo
$ rltbl redo

$ rltbl undo
$ rltbl undo
$ rltbl undo
$ rltbl redo
$ rltbl redo
$ rltbl redo

$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl redo
$ rltbl redo

$ rltbl undo
$ rltbl redo

$ rltbl redo

$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Delete row 13 (action #24, undo)
  Delete row 12 (action #25, undo)
▲ Delete row 11 (action #26, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl move row penguin 9 7
$ rltbl undo
$ rltbl set value penguin 4 island Enderby
$ rltbl delete row penguin 9
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Add row 9 after row 8 (action #6, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #7, undo)
▲ Delete row 11 (action #8, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ rltbl set value penguin 4 island Enderby
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl delete row penguin 9
$ rltbl set value penguin 3 species Godzilla
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 3 5
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Update 'species' in row 3 from Godzilla to Pygoscelis adeliae (action #11, undo)
▲ Add row 9 after row 8 (action #12, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ rltbl delete row penguin 5
$ rltbl undo

$ rltbl delete row penguin 10
$ rltbl undo

$ rltbl redo

$ rltbl move row penguin 9 7
$ rltbl move row penguin 4 8

$ rltbl undo
$ rltbl redo

$ rltbl undo
$ rltbl undo

$ rltbl redo
$ rltbl redo

$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Move row 4 from after row 8 to after row 3 (action #14, undo)
  Move row 9 from after row 7 to after row 8 (action #15, undo)
▲ Add row 10 after row 9 (action #16, undo)
```

```console tesh-session="test"
$ rltbl demo --size 20 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ rltbl delete row penguin 1
$ rltbl undo

$ rltbl delete row penguin 3
$ rltbl delete row penguin 7
$ rltbl undo
$ rltbl undo

$ rltbl redo
$ rltbl undo

$ rltbl redo
$ rltbl redo
    
$ rltbl undo
$ rltbl undo

$ rltbl redo
$ rltbl redo

$ rltbl undo
$ rltbl redo

$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Add row 7 after row 6 (action #17, undo)
▲ Add row 3 after row 2 (action #18, undo)
```
 
```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl delete row penguin 6
$ rltbl set value penguin 4 island Enderby
$ rltbl move row penguin 1 8
$ rltbl undo # Undo move row
$ rltbl undo # Undo set value
$ rltbl undo # Undo delete row
$ rltbl undo # Undo add row
$ rltbl redo
$ rltbl redo
$ rltbl redo
$ rltbl redo
$ rltbl undo
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Move row 1 from after row 8 to after row 0 (action #15, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #16, undo)
  Add row 6 after row 5 (action #17, undo)
▲ Delete row 11 (action #18, undo)
```
 
```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 12 1
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl move row penguin 4 9
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 3 1
$ rltbl move row penguin 4 2
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl get table penguin > penguin.tsv
$ rltbl delete row penguin 6
$ rltbl undo
$ rltbl redo

$ rltbl delete row penguin 9
$ rltbl undo
$ rltbl redo

$ rltbl undo
$ rltbl undo

$ rltbl get table penguin | diff - penguin.tsv
$ rltbl history
  Add row 9 after row 8 (action #7, undo)
▲ Add row 6 after row 5 (action #8, undo)
```
