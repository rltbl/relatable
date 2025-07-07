```console tesh-session="test"
$ export RLTBL_CONNECTION=postgresql:///rltbl_db
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force
Created a demonstration database in 'postgresql:///rltbl_db'
$ echo '{"species": "", "island": "", "sample_number": 20}' | rltbl --input JSON add row penguin
$ rltbl set value penguin 10 species ""
$ rltbl set value penguin 10 island ""
$ rltbl get table penguin
Rows 1-11 of 11
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
FAKE123     10                                            N10            31.50          30.00         4521
            20
$ echo "select * from penguin where species is null and island = '' order by _order" | psql rltbl_db
 _id | _order | study_name | sample_number | species | island | individual_id | culmen_length | culmen_depth | body_mass
-----+--------+------------+---------------+---------+--------+---------------+---------------+--------------+-----------
  10 |  10000 | FAKE123    |            10 |         |        | N10           |          31.5 |           30 |      4521
  11 |  11000 |            |            20 |         |        |               |               |              |
(2 rows)

$ rltbl save
$ echo "drop table penguin cascade" | psql rltbl_db
NOTICE:  drop cascades to 2 other objects
DETAIL:  drop cascades to view penguin_default_view
drop cascades to view penguin_text_view
DROP TABLE
$ rltbl init --force
Initialized a relatable database in 'postgresql:///rltbl_db'
$ rltbl load table --validate penguin.tsv
$ echo "select * from penguin where species is null and island = '' order by _order" | psql rltbl_db
 _id | _order | study_name | sample_number | species | island | individual_id | culmen_length | culmen_depth | body_mass
-----+--------+------------+---------------+---------+--------+---------------+---------------+--------------+-----------
  10 |  10000 | FAKE123    |            10 |         |        | N10           |          31.5 |           30 |      4521
  11 |  11000 |            |            20 |         |        |               |               |              |
(2 rows)

$ mv penguin.tsv penguin.tsv.2
$ rltbl save
$ diff penguin.tsv penguin.tsv.2
```
