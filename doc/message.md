# Adding and removing messages

**rltbl** can be used to directly add and delete messages to and from the message table. The purpose of a message is to provide information about some problem, or something else of note, about a particular value of a particular column of a particular row in some table. Each message, in addition, must specify a **level**, a **rule**, and the **message** text and is associated with a particular user, which may be specified via the environment variable, `RLTBL_USER`.  Let's begin by adding two messages to the penguin table.

```console tesh-session="message"
$ rltbl -v demo --size 10 --force
Created a demonstration database in '.relatable/relatable.db'
$ echo '{"study_name": "FAKE123", "sample_number": "SAMPLE #11", "species": "Pygoscelis adeliae", "island": "Biscoe", "individual_id": "N11", "culmen_length": 35.4, "body_mass": 2001}' | rltbl --input JSON add row penguin
$ rltbl set value penguin 9 sample_number SAMPLE09
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
FAKE123     SAMPLE09       Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
FAKE123     SAMPLE #11     Pygoscelis adeliae  Biscoe     N11            35.40                        2001
$ sqlite3 -header .relatable/relatable.db 'select * from message order by message_id'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer

$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
```

The messages are not normally visible when viewing the table's contents on the command line, but by increasing **rltbl**'s verbosity level we can see more detail about the first few rows returned from a `get table` command:

```console tesh-session="message"
$ rltbl -v get table penguin
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
FAKE123     SAMPLE09       Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
FAKE123     SAMPLE #11     Pygoscelis adeliae  Biscoe     N11            35.40                        2001
```

In any case the messages have been added to the message table in the database:

```
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|mike|penguin|3|species|Pygoscelis adeliae|info|custom-a|this is not a good species
2|mike|penguin|4|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
```

We delete messages using `rltbl delete message TABLE [ROW] [COLUMN]`. If row is unspecified, all messages in the given table are deleted. If column is unspecified, all messages in the given row are deleted. You can also use the `--rule RULE` flag to further filter the messsages to be deleted so that only those whose rule matches the given string are actually deleted, as opposed to all of the messages in the given table, column, or row. Note that SQL wildcard characters are allowed. In the current example, the string `custom%` happens to match all of the rules input thus far:

```console tesh-session="message"
$ rltbl -v delete message penguin --rule custom%
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer
```

Let's add a few more messages to the message table. Two of them will be by the user **mike** and the rest by the user **afreen**.

```console tesh-session="message"
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 5 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 species
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 study_name
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 7 study_name
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer
5|mike|penguin|3|species|Pygoscelis adeliae|info|custom-a|this is not a good species
6|mike|penguin|4|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
7|afreen|penguin|5|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
8|afreen|penguin|6|species|Pygoscelis adeliae|info|custom-a|this is not a good species
9|afreen|penguin|6|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
10|afreen|penguin|7|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
```

Let's now delete all the messages added to the table by **mike** using the `--user USER` option (which does not permit wildcards):

```console tesh-session="message"
$ rltbl -v delete message penguin --user mike
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer
7|afreen|penguin|5|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
8|afreen|penguin|6|species|Pygoscelis adeliae|info|custom-a|this is not a good species
9|afreen|penguin|6|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
10|afreen|penguin|7|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
```

Now delete all messages associated with the column **species** in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6 species
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer
7|afreen|penguin|5|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
9|afreen|penguin|6|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
10|afreen|penguin|7|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
```

Delete any remaining messages in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|Valve|penguin|11|sample_number|SAMPLE #11|error|sql_type:integer|sample_number must be of type integer
2|Valve|penguin|9|sample_number|SAMPLE09|error|sql_type:integer|sample_number must be of type integer
7|afreen|penguin|5|species|Pygoscelis adeliae|info|custom-b|this is a terrible species
10|afreen|penguin|7|study_name|FAKE123|info|custom-c|this is an inappropriate study_name
```

Delete all remaining messages:

```console tesh-session="message"
$ rltbl -v delete message penguin
$ sqlite3 -header .relatable/relatable.db 'select * from message'

```
