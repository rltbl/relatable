This test is identical (as of 2025-04-13) to the test in `doc/messsage.md` but it is useful to have it here as well because only the test cases in the `test/` directory are automoatically run against a PostgreSQL database when we push a commit to GitHub. See the GitHub workflow for testing against Postgres in `.github/workflows` for information about how this is done.

```console tesh-session="test"
$ export RLTBL_CONNECTION=postgresql:///rltbl_db
$ alias rltbl='rltbl -v'
$ rltbl demo --size 10 --force
Created a demonstration database in 'postgresql:///rltbl_db'
$ echo '{"study_name": "FAKE123", "sample_number": "SAMPLE #11", "species": "Pygoscelis adeliae", "island": "Biscoe", "individual_id": "N6A1", "bill_length": 35.4, "body_mass": 2001}' | rltbl --input JSON add row penguin
$ rltbl set value penguin 9 sample_number SAMPLE09
$ rltbl get table penguin
Rows 1-11 of 11
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1A1           44.6         31.1        4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N1A2           30.5         33.4        3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N2A1           35.2         22.4        4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N2A2           34.3         35.8        3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N3A1           40.6         39.9        2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N3A2           30.9         22.2        4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N4A1           38.6         28.5        3607
FAKE123     8              Pygoscelis adeliae  Dream      N4A2           33.8         39.9        1908
FAKE123     SAMPLE09       Pygoscelis adeliae  Dream      N5A1           43.7         23.1        3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           31.5         30.0        4521
FAKE123     SAMPLE #11     Pygoscelis adeliae  Biscoe     N6A1           35.4                     2001
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |   value    | level |       rule       |                message
------------+----------+---------+-----+---------------+------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11 | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09   | error | sql_type:integer | sample_number must be of type integer
(2 rows)

$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 4 species
$ rltbl get table penguin
Rows 1-11 of 11
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1A1           44.6         31.1        4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N1A2           30.5         33.4        3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N2A1           35.2         22.4        4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N2A2           34.3         35.8        3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N3A1           40.6         39.9        2129
FAKE123     6              Pygoscelis adeliae  Biscoe     N3A2           30.9         22.2        4962
FAKE123     7              Pygoscelis adeliae  Biscoe     N4A1           38.6         28.5        3607
FAKE123     8              Pygoscelis adeliae  Dream      N4A2           33.8         39.9        1908
FAKE123     SAMPLE09       Pygoscelis adeliae  Dream      N5A1           43.7         23.1        3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           31.5         30.0        4521
FAKE123     SAMPLE #11     Pygoscelis adeliae  Biscoe     N6A1           35.4                     2001
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |       value        | level |       rule       |                message
------------+----------+---------+-----+---------------+--------------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11         | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09           | error | sql_type:integer | sample_number must be of type integer
          3 | mike     | penguin |   3 | species       | Pygoscelis adeliae | info  | custom-a         | this is not a good species
          4 | mike     | penguin |   4 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
(4 rows)

$ rltbl delete message penguin --rule custom%
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |   value    | level |       rule       |                message
------------+----------+---------+-----+---------------+------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11 | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09   | error | sql_type:integer | sample_number must be of type integer
(2 rows)

$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl --input JSON add message penguin 4 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 5 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 6 species
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 6 study_name
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl --input JSON add message penguin 7 study_name
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |       value        | level |       rule       |                message
------------+----------+---------+-----+---------------+--------------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11         | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09           | error | sql_type:integer | sample_number must be of type integer
          5 | mike     | penguin |   3 | species       | Pygoscelis adeliae | info  | custom-a         | this is not a good species
          6 | mike     | penguin |   4 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
          7 | afreen   | penguin |   5 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
          8 | afreen   | penguin |   6 | species       | Pygoscelis adeliae | info  | custom-a         | this is not a good species
          9 | afreen   | penguin |   6 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
         10 | afreen   | penguin |   7 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
(8 rows)

$ rltbl delete message penguin --user mike
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |       value        | level |       rule       |                message
------------+----------+---------+-----+---------------+--------------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11         | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09           | error | sql_type:integer | sample_number must be of type integer
          7 | afreen   | penguin |   5 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
          8 | afreen   | penguin |   6 | species       | Pygoscelis adeliae | info  | custom-a         | this is not a good species
          9 | afreen   | penguin |   6 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
         10 | afreen   | penguin |   7 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
(6 rows)

$ rltbl delete message penguin 6 species
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |       value        | level |       rule       |                message
------------+----------+---------+-----+---------------+--------------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11         | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09           | error | sql_type:integer | sample_number must be of type integer
          7 | afreen   | penguin |   5 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
          9 | afreen   | penguin |   6 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
         10 | afreen   | penguin |   7 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
(5 rows)

$ rltbl delete message penguin 6
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by |  table  | row |    column     |       value        | level |       rule       |                message
------------+----------+---------+-----+---------------+--------------------+-------+------------------+---------------------------------------
          1 | rltbl    | penguin |  11 | sample_number | SAMPLE #11         | error | sql_type:integer | sample_number must be of type integer
          2 | rltbl    | penguin |   9 | sample_number | SAMPLE09           | error | sql_type:integer | sample_number must be of type integer
          7 | afreen   | penguin |   5 | species       | Pygoscelis adeliae | info  | custom-b         | this is a terrible species
         10 | afreen   | penguin |   7 | study_name    | FAKE123            | info  | custom-c         | this is an inappropriate study_name
(4 rows)

$ rltbl delete message penguin
$ echo 'select * from message order by message_id' | psql rltbl_db
 message_id | added_by | table | row | column | value | level | rule | message
------------+----------+-------+-----+--------+-------+-------+------+---------
(0 rows)

```
