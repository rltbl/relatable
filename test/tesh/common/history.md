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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
FAKE123     11             Pygoscelis adeliae  Torgersen  N11            39.50          37.50         4174
FAKE123     12             Pygoscelis adeliae  Torgersen  N12            44.60          21.20         4700
FAKE123     13             Pygoscelis adeliae  Biscoe     N13            34.30          28.70         4908
FAKE123     14             Pygoscelis adeliae  Dream      N14            43.50          20.30         4274
FAKE123     15             Pygoscelis adeliae  Biscoe     N15            47.10          32.30         3803
FAKE123     16             Pygoscelis adeliae  Torgersen  N16            45.70          33.30         4458
FAKE123     17             Pygoscelis adeliae  Biscoe     N17            46.30          30.30         4444
FAKE123     18             Pygoscelis adeliae  Torgersen  N18            47.30          23.30         1350
FAKE123     19             Pygoscelis adeliae  Biscoe     N19            37.00          37.90         1749
FAKE123     20             Pygoscelis adeliae  Torgersen  N20            40.40          32.40         4906
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
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
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.90          22.20         4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
$ rltbl history
  Add row 9 after row 8 (action #7, undo)
▲ Add row 6 after row 5 (action #8, undo)
```
