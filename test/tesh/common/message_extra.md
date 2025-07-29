```console tesh-session="test"
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force
Created a demonstration database in '...'
$ rltbl set value island 3 island Montreal
$ rltbl get table penguin --format vertical
penguin
-----
study_name: FAKE123
sample_number: 1
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A1
bill_length: 44.6
bill_depth: 31.1
body_mass: 4093
-----
study_name: FAKE123
sample_number: 2
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A2
bill_length: 30.5
bill_depth: 33.4
body_mass: 3336
-----
study_name: FAKE123
sample_number: 3
species: Pygoscelis adeliae
island: Torgersen
individual_id: N2A1
bill_length: 35.2
bill_depth: 22.4
body_mass: 4087
-----
study_name: FAKE123
sample_number: 4
species: Pygoscelis adeliae
island: Biscoe
individual_id: N2A2
bill_length: 34.3
bill_depth: 35.8
body_mass: 3469
-----
study_name: FAKE123
sample_number: 5
species: Pygoscelis adeliae
island: Torgersen
individual_id: N3A1
bill_length: 40.6
bill_depth: 39.9
body_mass: 2129
-----
study_name: FAKE123
sample_number: 6
species: Pygoscelis adeliae
island: Biscoe
individual_id: N3A2
bill_length: 30.9
bill_depth: 22.2
body_mass: 4962
-----
study_name: FAKE123
sample_number: 7
species: Pygoscelis adeliae
island: Biscoe
individual_id: N4A1
bill_length: 38.6
bill_depth: 28.5
body_mass: 3607
-----
study_name: FAKE123
sample_number: 8
species: Pygoscelis adeliae
island: Dream [error (key:foreign): island must be in island.island]
individual_id: N4A2
bill_length: 33.8
bill_depth: 39.9
body_mass: 1908
-----
study_name: FAKE123
sample_number: 9
species: Pygoscelis adeliae
island: Dream [error (key:foreign): island must be in island.island]
individual_id: N5A1
bill_length: 43.7
bill_depth: 23.1
body_mass: 3883
-----
study_name: FAKE123
sample_number: 10
species: Pygoscelis adeliae
island: Torgersen
individual_id: N5A2
bill_length: 31.5
bill_depth: 30.0
body_mass: 4521
-----
$ rltbl set value island 3 island Dream
$ rltbl get table penguin --format vertical
penguin
-----
study_name: FAKE123
sample_number: 1
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A1
bill_length: 44.6
bill_depth: 31.1
body_mass: 4093
-----
study_name: FAKE123
sample_number: 2
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A2
bill_length: 30.5
bill_depth: 33.4
body_mass: 3336
-----
study_name: FAKE123
sample_number: 3
species: Pygoscelis adeliae
island: Torgersen
individual_id: N2A1
bill_length: 35.2
bill_depth: 22.4
body_mass: 4087
-----
study_name: FAKE123
sample_number: 4
species: Pygoscelis adeliae
island: Biscoe
individual_id: N2A2
bill_length: 34.3
bill_depth: 35.8
body_mass: 3469
-----
study_name: FAKE123
sample_number: 5
species: Pygoscelis adeliae
island: Torgersen
individual_id: N3A1
bill_length: 40.6
bill_depth: 39.9
body_mass: 2129
-----
study_name: FAKE123
sample_number: 6
species: Pygoscelis adeliae
island: Biscoe
individual_id: N3A2
bill_length: 30.9
bill_depth: 22.2
body_mass: 4962
-----
study_name: FAKE123
sample_number: 7
species: Pygoscelis adeliae
island: Biscoe
individual_id: N4A1
bill_length: 38.6
bill_depth: 28.5
body_mass: 3607
-----
study_name: FAKE123
sample_number: 8
species: Pygoscelis adeliae
island: Dream
individual_id: N4A2
bill_length: 33.8
bill_depth: 39.9
body_mass: 1908
-----
study_name: FAKE123
sample_number: 9
species: Pygoscelis adeliae
island: Dream
individual_id: N5A1
bill_length: 43.7
bill_depth: 23.1
body_mass: 3883
-----
study_name: FAKE123
sample_number: 10
species: Pygoscelis adeliae
island: Torgersen
individual_id: N5A2
bill_length: 31.5
bill_depth: 30.0
body_mass: 4521
-----
$ rltbl set value island 3 island Montreal
$ rltbl save
$ rltbl drop database
$ rltbl demo --size 0 --force
Created a demonstration database in '...'
$ rltbl load table island.tsv --force
$ rltbl load table penguin.tsv --force
$ rltbl get table penguin --format vertical
penguin
-----
study_name: FAKE123
sample_number: 1
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A1
bill_length: 44.6
bill_depth: 31.1
body_mass: 4093
-----
study_name: FAKE123
sample_number: 2
species: Pygoscelis adeliae
island: Torgersen
individual_id: N1A2
bill_length: 30.5
bill_depth: 33.4
body_mass: 3336
-----
study_name: FAKE123
sample_number: 3
species: Pygoscelis adeliae
island: Torgersen
individual_id: N2A1
bill_length: 35.2
bill_depth: 22.4
body_mass: 4087
-----
study_name: FAKE123
sample_number: 4
species: Pygoscelis adeliae
island: Biscoe
individual_id: N2A2
bill_length: 34.3
bill_depth: 35.8
body_mass: 3469
-----
study_name: FAKE123
sample_number: 5
species: Pygoscelis adeliae
island: Torgersen
individual_id: N3A1
bill_length: 40.6
bill_depth: 39.9
body_mass: 2129
-----
study_name: FAKE123
sample_number: 6
species: Pygoscelis adeliae
island: Biscoe
individual_id: N3A2
bill_length: 30.9
bill_depth: 22.2
body_mass: 4962
-----
study_name: FAKE123
sample_number: 7
species: Pygoscelis adeliae
island: Biscoe
individual_id: N4A1
bill_length: 38.6
bill_depth: 28.5
body_mass: 3607
-----
study_name: FAKE123
sample_number: 8
species: Pygoscelis adeliae
island: Dream [error (key:foreign): island must be in island.island]
individual_id: N4A2
bill_length: 33.8
bill_depth: 39.9
body_mass: 1908
-----
study_name: FAKE123
sample_number: 9
species: Pygoscelis adeliae
island: Dream [error (key:foreign): island must be in island.island]
individual_id: N5A1
bill_length: 43.7
bill_depth: 23.1
body_mass: 3883
-----
study_name: FAKE123
sample_number: 10
species: Pygoscelis adeliae
island: Torgersen
individual_id: N5A2
bill_length: 31.5
bill_depth: 30.0
body_mass: 4521
-----
```
