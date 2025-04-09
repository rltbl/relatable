```console tesh-session="test"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "", "island": ""}' | rltbl --input JSON add row penguin
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
FAKE123     10             null                           N10            33.8           4697
null        null           null                           null           null           null
$ sqlite3 -header .relatable/relatable.db "select * from penguin where species is null and island = ''"
_id|_order|study_name|sample_number|species|island|individual_id|culmen_length|body_mass
10|10000|FAKE123|10|||N10|33.8|4697
11|11000|||||||
$ rltbl save
$ sqlite3 .relatable/relatable.db "drop table penguin"
$ sqlite3 .relatable/relatable.db "delete from \"table\" where \"table\" like 'penguin'"
$ rltbl load table penguin.tsv
$ sqlite3 -header .relatable/relatable.db "select * from penguin where species is null and island = ''"
_id|_order|study_name|sample_number|species|island|individual_id|culmen_length|body_mass
10|10000|FAKE123|10|||N10|33.8|4697
11|11000|||||||
$ mv penguin.tsv penguin.tsv.2
$ rltbl save
$ diff penguin.tsv penguin.tsv.2
```
