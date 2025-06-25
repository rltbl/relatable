```console tesh-session="test"
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force
Created a demonstration database in '.relatable/relatable.db'
$ echo '{"species": "", "island": "", "sample_number": 20}' | rltbl --input JSON add row penguin
$ rltbl set value penguin 10 species ""
$ rltbl set value penguin 10 island ""
$ rltbl get table penguin
Rows 1-11 of 11
study_name  sample_number  species             island     individual_id  culmen_length       culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.599998474121094  31.1          4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.5                33.4          3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20000076293945   22.4          4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N4             34.29999923706055   35.8          3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.599998474121094  39.9          2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N6             30.899999618530273  22.2          4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.599998474121094  28.5          3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.79999923706055   39.9          1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70000076293945   23.1          3883
FAKE123     10                                            N10            31.5                30...         4521
            20
$ sqlite3 -header .relatable/relatable.db "select * from penguin where species is null and island = '' order by _order"
_id|_order|study_name|sample_number|species|island|individual_id|culmen_length|culmen_depth|body_mass
10|10000|FAKE123|10|||N10|31.5|30|4521
11|11000||20||||||
$ rltbl save
$ sqlite3 .relatable/relatable.db "drop table penguin"
$ sqlite3 .relatable/relatable.db "delete from \"table\" where \"table\" like 'penguin'"
$ rltbl load table --validate penguin.tsv
$ sqlite3 -header .relatable/relatable.db "select * from penguin where species is null and island = '' order by _order"
_id|_order|study_name|sample_number|species|island|individual_id|culmen_length|culmen_depth|body_mass
10|10000|FAKE123|10|||N10|31.5|30|4521
11|11000||20||||||
$ mv penguin.tsv penguin.tsv.2
$ rltbl save
$ diff penguin.tsv penguin.tsv.2
```
