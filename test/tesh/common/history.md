```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force | diff - expected_output.txt
$ rm -f expected_output.txt
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'sample_number' in row 4 from 26 to 4 (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Delete row 13 (action #24, undo)
  Delete row 12 (action #25, undo)
▲ Delete row 11 (action #26, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl move row penguin 9 7
$ rltbl undo
$ rltbl set value penguin 4 island Enderby
$ rltbl delete row penguin 9
$ rltbl undo
$ rltbl undo
$ rltbl undo

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Add row 9 after row 8 (action #6, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #7, undo)
▲ Delete row 11 (action #8, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Update 'species' in row 3 from Godzilla to Pygoscelis adeliae (action #11, undo)
▲ Add row 9 after row 8 (action #12, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Move row 4 from after row 8 to after row 3 (action #14, undo)
  Move row 9 from after row 7 to after row 8 (action #15, undo)
▲ Add row 10 after row 9 (action #16, undo)
```

```console tesh-session="test"
$ rltbl demo --size 20 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
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
$ rltbl history
  Add row 7 after row 6 (action #17, undo)
▲ Add row 3 after row 2 (action #18, undo)
```
 
```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Move row 1 from after row 8 to after row 0 (action #15, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #16, undo)
  Add row 6 after row 5 (action #17, undo)
▲ Delete row 11 (action #18, undo)
```
 
```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
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

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

```console tesh-session="test"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ rltbl delete row penguin 6
$ rltbl undo
$ rltbl redo

$ rltbl delete row penguin 9
$ rltbl undo
$ rltbl redo

$ rltbl undo
$ rltbl undo

$ rltbl get table penguin
Rows 1-10 of 10
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
$ rltbl history
  Add row 9 after row 8 (action #7, undo)
▲ Add row 6 after row 5 (action #8, undo)
```
