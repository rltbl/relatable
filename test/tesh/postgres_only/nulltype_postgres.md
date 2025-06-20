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
FAKE123     10                                            N10            33.8           4697
            20                                                                              
$ echo "select * from penguin where species is null and island = '' order by _order" | psql rltbl_db
 _id | _order | study_name | sample_number | species | island | individual_id | culmen_length | body_mass 
-----+--------+------------+---------------+---------+--------+---------------+---------------+-----------
  10 |  10000 | FAKE123    |            10 |         |        | N10           | 33.8          | 4697
  11 |  11000 |            |            20 |         |        |               |               | 
(2 rows)

$ rltbl save
$ echo "drop table penguin cascade" | psql rltbl_db
NOTICE:  drop cascades to 2 other objects
DETAIL:  drop cascades to view penguin_default_view
drop cascades to view penguin_text_view
DROP TABLE
$ rltbl init --force
Initialized a relatable database in 'postgresql:///rltbl_db'
$ rltbl load table penguin.tsv
$ echo "select * from penguin where species is null and island = '' order by _order" | psql rltbl_db
 _id | _order | study_name | sample_number | species | island | individual_id | culmen_length | body_mass 
-----+--------+------------+---------------+---------+--------+---------------+---------------+-----------
  10 |  10000 | FAKE123    |            10 |         |        | N10           | 33.8          | 4697
  11 |  11000 |            |            20 |         |        |               |               | 
(2 rows)

$ mv penguin.tsv penguin.tsv.2
$ rltbl save
$ diff penguin.tsv penguin.tsv.2
```
