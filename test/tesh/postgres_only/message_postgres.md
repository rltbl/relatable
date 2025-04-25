This test is identical (as of 2025-04-13) to the test in `doc/messsage.md` but it is useful to have it here as well because only the test cases in the `test/` directory are automoatically run against a PostgreSQL database when we push a commit to GitHub. See the GitHub workflow for testing against Postgres in `.github/workflows` for information about how this is done.

```console tesh-session="test"
$ export RLTBL_CONNECTION=postgresql:///rltbl_db
$ rltbl demo --size 10 --force
Created a demonstration database in 'postgresql:///rltbl_db'
$ echo '{"level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 3 species
$ echo '{"level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 4 species
$ rltbl -vv get table penguin
... INFO rltbl::core: Received 10 rows, of which the first 4 are: [
    {"_id": "1", "_order": "1000", "_history": "", "_message": "", "study_name": "FAKE123", "sample_number": "1", "species": "Pygoscelis adeliae", "island": "Torgersen", "individual_id": "N1", "culmen_length": "44.6", "body_mass": "3221"},
    {"_id": "2", "_order": "2000", "_history": "", "_message": "", "study_name": "FAKE123", "sample_number": "2", "species": "Pygoscelis adeliae", "island": "Torgersen", "individual_id": "N2", "culmen_length": "30.5", "body_mass": "3685"},
    {"_id": "3", "_order": "3000", "_history": "", "_message": "[{\"column\":\"species\",\"value\":\"Pygoscelis adeliae\",\"level\":\"error\",\"rule\":\"custom-a\",\"message\":\"this is not a good species\"}]", "study_name": "FAKE123", "sample_number": "3", "species": "Pygoscelis adeliae", "island": "Torgersen", "individual_id": "N3", "culmen_length": "35.2", "body_mass": "1491"},
    {"_id": "4", "_order": "4000", "_history": "", "_message": "[{\"column\":\"species\",\"value\":\"Pygoscelis adeliae\",\"level\":\"error\",\"rule\":\"custom-b\",\"message\":\"this is a terrible species\"}]", "study_name": "FAKE123", "sample_number": "4", "species": "Pygoscelis adeliae", "island": "Torgersen", "individual_id": "N4", "culmen_length": "31.4", "body_mass": "1874"},
]
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
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row | column  |       value        | level |   rule   |          message
------------+----------+---------+-----+---------+--------------------+-------+----------+----------------------------
          1 | mike     | penguin |   3 | species | Pygoscelis adeliae | error | custom-a | this is not a good species
          2 | mike     | penguin |   4 | species | Pygoscelis adeliae | error | custom-b | this is a terrible species
(2 rows)

$ rltbl delete message penguin --rule custom%
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by | table | row | column | value | level | rule | message
------------+----------+-------+-----+--------+-------+-------+------+---------
(0 rows)

$ echo '{"level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 3 species
$ echo '{"level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 4 species
$ echo '{"level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 5 species
$ echo '{"level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 6 species
$ echo '{"level": "error", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 6 study_name
$ echo '{"level": "error", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 7 study_name
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |   column   |       value        | level |   rule   |               message
------------+----------+---------+-----+------------+--------------------+-------+----------+-------------------------------------
          3 | mike     | penguin |   3 | species    | Pygoscelis adeliae | error | custom-a | this is not a good species
          4 | mike     | penguin |   4 | species    | Pygoscelis adeliae | error | custom-b | this is a terrible species
          5 | afreen   | penguin |   5 | species    | Pygoscelis adeliae | error | custom-b | this is a terrible species
          6 | afreen   | penguin |   6 | species    | Pygoscelis adeliae | error | custom-a | this is not a good species
          7 | afreen   | penguin |   6 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
          8 | afreen   | penguin |   7 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
(6 rows)

$ rltbl delete message penguin --user mike
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |   column   |       value        | level |   rule   |               message
------------+----------+---------+-----+------------+--------------------+-------+----------+-------------------------------------
          5 | afreen   | penguin |   5 | species    | Pygoscelis adeliae | error | custom-b | this is a terrible species
          6 | afreen   | penguin |   6 | species    | Pygoscelis adeliae | error | custom-a | this is not a good species
          7 | afreen   | penguin |   6 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
          8 | afreen   | penguin |   7 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
(4 rows)

$ rltbl delete message penguin 6 species
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |   column   |       value        | level |   rule   |               message
------------+----------+---------+-----+------------+--------------------+-------+----------+-------------------------------------
          5 | afreen   | penguin |   5 | species    | Pygoscelis adeliae | error | custom-b | this is a terrible species
          7 | afreen   | penguin |   6 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
          8 | afreen   | penguin |   7 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
(3 rows)

$ rltbl delete message penguin 6
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |   column   |       value        | level |   rule   |               message
------------+----------+---------+-----+------------+--------------------+-------+----------+-------------------------------------
          5 | afreen   | penguin |   5 | species    | Pygoscelis adeliae | error | custom-b | this is a terrible species
          8 | afreen   | penguin |   7 | study_name | FAKE123            | error | custom-c | this is an inappropriate study_name
(2 rows)

$ rltbl delete message penguin
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by | table | row | column | value | level | rule | message
------------+----------+-------+-----+--------+-------+-------+------+---------
(0 rows)

```
