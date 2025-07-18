# Adding and removing messages

**rltbl** can be used to directly add and delete messages to and from the message table. The purpose of a message is to provide information about some problem, or something else of note, about a particular value of a particular column of a particular row in some table. Each message, in addition, must specify a **level**, a **rule**, and the **message** text and is associated with a particular user, which may be specified via the environment variable, `RLTBL_USER`.  Let's begin by adding two messages to the penguin table.

```console tesh-session="message"
$ rltbl -v demo --size 10 --force
Created a demonstration database in ...
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
$ rltbl get table message
Rows 1-2 of 2
message_id  added_by  table    row  column         value       level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11  error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09    error  datatype:integer  sample_number must be of type integer
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
```

The messages are not normally visible when viewing the table's contents on the command line, but by increasing **rltbl**'s verbosity level we can see more detail about the first few rows returned from a `get table` command:

```console tesh-session="message"
$ rltbl -v get table penguin
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
```

In any case the messages have been added to the message table in the database:

```
message_id  added_by  table    row   column         value       level  rule              message
1           rltbl     penguin  1001  sample_number  SAMPLE #11  error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9     sample_number  SAMPLE09    error  datatype:integer  sample_number must be of type integer
```

We delete messages using `rltbl delete message TABLE [ROW] [COLUMN]`. If row is unspecified, all messages in the given table are deleted. If column is unspecified, all messages in the given row are deleted. You can also use the `--rule RULE` flag to further filter the messsages to be deleted so that only those whose rule matches the given string are actually deleted, as opposed to all of the messages in the given table, column, or row. Note that SQL wildcard characters are allowed. In the current example, the string `custom%` happens to match all of the rules input thus far:

```console tesh-session="message"
$ rltbl -v delete message penguin --rule custom%
$ rltbl get table message
Rows 1-2 of 2
message_id  added_by  table    row  column         value       level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11  error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09    error  datatype:integer  sample_number must be of type integer
```

Let's add a few more messages to the message table. Two of them will be by the user **mike** and the rest by the user **afreen**.

```console tesh-session="message"
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 5 species
$ echo '{"value": "Pygoscelis adeliae", "level": "info", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 species
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 study_name
$ echo '{"value": "FAKE123", "level": "info", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 7 study_name
$ rltbl get table message
Rows 1-8 of 8
message_id  added_by  table    row  column         value               level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11          error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09            error  datatype:integer  sample_number must be of type integer
5           mike      penguin  3    species        Pygoscelis adeliae  info   custom-a          this is not a good species
6           mike      penguin  4    species        Pygoscelis adeliae  info   custom-b          this is a terrible species
7           afreen    penguin  5    species        Pygoscelis adeliae  info   custom-b          this is a terrible species
8           afreen    penguin  6    species        Pygoscelis adeliae  info   custom-a          this is not a good species
9           afreen    penguin  6    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
10          afreen    penguin  7    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
```

Let's now delete all the messages added to the table by **mike** using the `--user USER` option (which does not permit wildcards):

```console tesh-session="message"
$ rltbl -v delete message penguin --user mike
$ rltbl get table message
Rows 1-6 of 6
message_id  added_by  table    row  column         value               level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11          error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09            error  datatype:integer  sample_number must be of type integer
7           afreen    penguin  5    species        Pygoscelis adeliae  info   custom-b          this is a terrible species
8           afreen    penguin  6    species        Pygoscelis adeliae  info   custom-a          this is not a good species
9           afreen    penguin  6    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
10          afreen    penguin  7    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
```

Now delete all messages associated with the column **species** in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6 species
$ rltbl get table message
Rows 1-5 of 5
message_id  added_by  table    row  column         value               level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11          error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09            error  datatype:integer  sample_number must be of type integer
7           afreen    penguin  5    species        Pygoscelis adeliae  info   custom-b          this is a terrible species
9           afreen    penguin  6    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
10          afreen    penguin  7    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
```

Delete any remaining messages in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6
$ rltbl get table message
Rows 1-4 of 4
message_id  added_by  table    row  column         value               level  rule              message
1           rltbl     penguin  11   sample_number  SAMPLE #11          error  datatype:integer  sample_number must be of type integer
2           rltbl     penguin  9    sample_number  SAMPLE09            error  datatype:integer  sample_number must be of type integer
7           afreen    penguin  5    species        Pygoscelis adeliae  info   custom-b          this is a terrible species
10          afreen    penguin  7    study_name     FAKE123             info   custom-c          this is an inappropriate study_name
```

Delete all remaining messages:

```console tesh-session="message"
$ rltbl -v delete message penguin
$ rltbl get table message
Rows 1-0 of 0
message_id  added_by  table  row  column  value  level  rule  message
```
