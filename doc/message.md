# Adding and removing messages

**rltbl** can be used to directly add and delete messages to and from the message table. The purpose of a message is to provide information about some problem, or something else of note, about a particular value of a particular column of a particular row in some table. Each message, in addition, must specify a **level**, a **rule**, and the **message** text and is associated with a particular user, which may be specified via the environment variable, `RLTBL_USER`.  Let's begin by adding two messages to the penguin table.

```console tesh-session="message"
$ rltbl -v demo --size 10 --force
Created a demonstration database in '.relatable/relatable.db'
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
```

The messages are not normally visible when viewing the table's contents on the command line, but by increasing **rltbl**'s verbosity level we can see more detail about the first few rows returned from a `get table` command:

```console tesh-session="message"
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
```

In any case the messages have been added to the message table in the database:

```
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
1|mike|penguin|3|species|Pygoscelis adeliae|error|custom-a|this is not a good species
2|mike|penguin|4|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
```

We delete messages using `rltbl delete message TABLE [ROW] [COLUMN]`. If row is unspecified, all messages in the given table are deleted. If column is unspecified, all messages in the given row are deleted. You can also use the `--rule RULE` flag to further filter the messsages to be deleted so that only those whose rule matches the given string are actually deleted, as opposed to all of the messages in the given table, column, or row. Note that SQL wildcard characters are allowed. In the current example, the string `custom%` happens to match all of the rules input thus far:

```console tesh-session="message"
$ rltbl -v delete message penguin --rule custom%
$ sqlite3 -header .relatable/relatable.db 'select * from message'

```

Let's add a few more messages to the message table. Two of them will be by the user **mike** and the rest by the user **afreen**.

```console tesh-session="message"
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 3 species
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=mike rltbl -v --input JSON add message penguin 4 species
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-b", "message": "this is a terrible species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 5 species
$ echo '{"value": "Pygoscelis adeliae", "level": "error", "rule": "custom-a", "message": "this is not a good species"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 species
$ echo '{"value": "FAKE123", "level": "error", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 6 study_name
$ echo '{"value": "FAKE123", "level": "error", "rule": "custom-c", "message": "this is an inappropriate study_name"}' | RLTBL_USER=afreen rltbl -v --input JSON add message penguin 7 study_name
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
3|mike|penguin|3|species|Pygoscelis adeliae|error|custom-a|this is not a good species
4|mike|penguin|4|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
5|afreen|penguin|5|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
6|afreen|penguin|6|species|Pygoscelis adeliae|error|custom-a|this is not a good species
7|afreen|penguin|6|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
8|afreen|penguin|7|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
```

Let's now delete all the messages added to the table by **mike** using the `--user USER` option (which does not permit wildcards):

```console tesh-session="message"
$ rltbl -v delete message penguin --user mike
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
5|afreen|penguin|5|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
6|afreen|penguin|6|species|Pygoscelis adeliae|error|custom-a|this is not a good species
7|afreen|penguin|6|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
8|afreen|penguin|7|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
```

Now delete all messages associated with the column **species** in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6 species
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
5|afreen|penguin|5|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
7|afreen|penguin|6|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
8|afreen|penguin|7|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
```

Delete any remaining messages in row 6:

```console tesh-session="message"
$ rltbl -v delete message penguin 6
$ sqlite3 -header .relatable/relatable.db 'select * from message'
message_id|added_by|table|row|column|value|level|rule|message
5|afreen|penguin|5|species|Pygoscelis adeliae|error|custom-b|this is a terrible species
8|afreen|penguin|7|study_name|FAKE123|error|custom-c|this is an inappropriate study_name
```

Delete all remaining messages:

```console tesh-session="message"
$ rltbl -v delete message penguin
$ sqlite3 -header .relatable/relatable.db 'select * from message'

```
