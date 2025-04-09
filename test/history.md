Test case 1

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v delete row penguin 6
$ rltbl -v set value penguin 4 island Enderby
$ rltbl -v move row penguin 1 8
$ rltbl -v undo # Undo move row
$ rltbl -v undo # Undo set value
$ rltbl -v undo # Undo delete row
$ rltbl -v undo # Undo add row

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'island' in row 4 from Enderby to Torgersen (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

Test case 2

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

Test case 3

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v move row penguin 12 1
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

Test case 4

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v move row penguin 4 9
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v move row penguin 3 1
$ rltbl -v move row penguin 4 2
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

Test case 5

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v redo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v redo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Delete row 13 (action #24, undo)
  Delete row 12 (action #25, undo)
▲ Delete row 11 (action #26, undo)
```

Test case 6

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ rltbl -v move row penguin 9 7
$ rltbl -v undo
$ rltbl -v set value penguin 4 island Enderby
$ rltbl -v delete row penguin 9
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Add row 9 after row 8 (action #6, undo)
  Update 'island' in row 4 from Enderby to Torgersen (action #7, undo)
▲ Delete row 11 (action #8, undo)
```

Test case 7

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ rltbl -v set value penguin 4 island Enderby
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v delete row penguin 9
$ rltbl -v set value penguin 3 species Godzilla
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v move row penguin 3 5
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Update 'species' in row 3 from Godzilla to Pygoscelis adeliae (action #11, undo)
▲ Add row 9 after row 8 (action #12, undo)
```

Test case 9

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ rltbl -v delete row penguin 5
$ rltbl -v undo

$ rltbl -v delete row penguin 10
$ rltbl -v undo

$ rltbl -v redo

$ rltbl -v move row penguin 9 7
$ rltbl -v move row penguin 4 8

$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v redo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Move row 4 from after row 8 to after row 3 (action #14, undo)
  Move row 9 from after row 7 to after row 8 (action #15, undo)
▲ Add row 10 after row 9 (action #16, undo)
```

Test case 10

```console tesh-session="test"
$ rltbl -v demo --size 30 --force
$ rltbl -v delete row penguin 1
$ rltbl -v undo

$ rltbl -v delete row penguin 3
$ rltbl -v delete row penguin 7
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v redo
$ rltbl -v undo

$ rltbl -v redo
$ rltbl -v redo
    
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v redo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-30 of 30
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
FAKE123     21             Pygoscelis adeliae  Torgersen  N21            45.7           2299
FAKE123     22             Pygoscelis adeliae  Torgersen  N22            46.3           1335
FAKE123     23             Pygoscelis adeliae  Dream      N23            47.2           4011
FAKE123     24             Pygoscelis adeliae  Torgersen  N24            33.3           1350
FAKE123     25             Pygoscelis adeliae  Biscoe     N25            37.0           4591
FAKE123     26             Pygoscelis adeliae  Biscoe     N26            47.7           3089
FAKE123     27             Pygoscelis adeliae  Dream      N27            35.2           2613
FAKE123     28             Pygoscelis adeliae  Dream      N28            43.3           1388
FAKE123     29             Pygoscelis adeliae  Torgersen  N29            42.4           4167
FAKE123     30             Pygoscelis adeliae  Torgersen  N30            35.8           4211
$ rltbl -v history
  Add row 7 after row 6 (action #17, undo)
▲ Add row 3 after row 2 (action #18, undo)
```
 Test case 11
 
```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v delete row penguin 6
$ rltbl -v set value penguin 4 island Enderby
$ rltbl -v move row penguin 1 8
$ rltbl -v undo # Undo move row
$ rltbl -v undo # Undo set value
$ rltbl -v undo # Undo delete row
$ rltbl -v undo # Undo add row
$ rltbl -v redo
$ rltbl -v redo
$ rltbl -v redo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Move row 1 from after row 8 to after row 0 (action #15, undo)
  Update 'island' in row 4 from Enderby to Torgersen (action #16, undo)
  Add row 6 after row 5 (action #17, undo)
▲ Delete row 11 (action #18, undo)
```
 Test case 12
 
```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Delete row 13 (action #8, undo)
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

Test case 13

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl -v --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v move row penguin 12 1
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Delete row 12 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

Test case 14

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
$ rltbl -v undo
$ rltbl -v move row penguin 4 9
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v move row penguin 3 1
$ rltbl -v move row penguin 4 2
$ rltbl -v undo
$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Move row 3 from after row 1 to after row 2 (action #9, undo)
▲ Move row 4 from after row 9 to after row 3 (action #10, undo)
```

Test case 15

```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ rltbl -v delete row penguin 6
$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v delete row penguin 9
$ rltbl -v undo
$ rltbl -v redo

$ rltbl -v undo
$ rltbl -v undo

$ rltbl -v get table penguin
Rows 1-10 of 10
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
$ rltbl -v history
  Add row 9 after row 8 (action #7, undo)
▲ Add row 6 after row 5 (action #8, undo)
```
