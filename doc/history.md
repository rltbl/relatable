# Editing your data

To add rows to a table on the command line, one uses `rltbl add row TABLE`. **rltbl** normally adds rows interactively by asking the user to supply a value for every column in the table in turn. Alternatively, the option `--input JSON` may be specified to accept the row to be added in the form a JSON-formatted string.

```console tesh-session="history" tesh-setup="setup.sh"
$ rltbl demo --size 10 --force
Created a demonstration database in ...
$ echo '{"species": "FOO"}' | RLTBL_USER=mike rltbl --input JSON add row penguin
```
Note the use of the environment variable, `RLTBL_USER`, to specify the user associated with this particular action. This can be done on a per-command basis, as we have done here, or alternately (the usual setup) one can set the environment variable in one's shell initialization script (e.g., `~/.bashrc`). Because the examples below depend sensitively on which actions are owned by which user, we have been careful to be explicit about the user of each command below for which it is relevant.

**rltbl** can undo and redo previous actions, and display the history of previous actions for this user.

```console tesh-session="history"
$ RLTBL_USER=mike rltbl undo
$ RLTBL_USER=mike rltbl redo
$ RLTBL_USER=mike rltbl delete row penguin 6
$ RLTBL_USER=mike rltbl set value penguin 4 island Enderby
$ RLTBL_USER=mike rltbl move row penguin 1 8
$ RLTBL_USER=mike rltbl undo
```

The contents of the penguin table are now:

```console tesh-session="history"
$ rltbl get table penguin
Rows 1-10 of 10
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1A1           44.6         31.1        4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N1A2           30.5         33.4        3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N2A1           35.2         22.4        4087
FAKE123     4              Pygoscelis adeliae  Enderby    N2A2           34.3         35.7        4174
FAKE123     5              Pygoscelis adeliae  Dream      N3A1           44.9         30.6        4875
FAKE123     7              Pygoscelis adeliae  Biscoe     N4A1           32.2         39.8        4859
FAKE123     8              Pygoscelis adeliae  Biscoe     N4A2           30.9         28.6        2702
FAKE123     9              Pygoscelis adeliae  Dream      N5A1           39.2         23.8        4697
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           34.5         27.9        3237
                           FOO                                                                    
```

We use the **history** subcommand to get information about the last few commands that can be undone or redone.
The line prefixed with a down-arrow, as well as those below it, can be undone. The other lines represent
undone changes that can  be redone.

```console tesh-session="history"
$ RLTBL_USER=mike rltbl history
Insert 1 row into "penguin" table
Delete 1 row from "penguin" table
Update 1 row in "penguin" table
▲ 3 changes can be undone
▼ 1 change can be redone
Move 1 row in "penguin" table
```

To restore the original state of the table we can finally do:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl undo # Undo set value
$ RLTBL_USER=mike rltbl undo # Undo delete row
$ RLTBL_USER=mike rltbl undo # Undo add row
$ RLTBL_USER=mike rltbl get table penguin
Rows 1-10 of 10
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1A1           44.6         31.1        4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N1A2           30.5         33.4        3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N2A1           35.2         22.4        4087
FAKE123     4              Pygoscelis adeliae  Biscoe     N2A2           34.3         35.7        4174
FAKE123     5              Pygoscelis adeliae  Dream      N3A1           44.9         30.6        4875
FAKE123     6              Pygoscelis adeliae  Torgersen  N3A2           49.9         25.6        1557
FAKE123     7              Pygoscelis adeliae  Biscoe     N4A1           32.2         39.8        4859
FAKE123     8              Pygoscelis adeliae  Biscoe     N4A2           30.9         28.6        2702
FAKE123     9              Pygoscelis adeliae  Dream      N5A1           39.2         23.8        4697
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           34.5         27.9        3237
```

As already mentioned, **rltbl** supports multiple users. It also supports and keeps track of multiple user histories. Although **mike**'s history currently looks like the following:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl history
▼ 4 changes can be redone
Insert 1 row into "penguin" table
Delete 1 row from "penguin" table
Update 1 row in "penguin" table
Move 1 row in "penguin" table
```

The same is not true of **afreen**, who has not yet performed any actions:

```console tesh-session="history"
$ RLTBL_USER=afreen rltbl history
No changes
```

If she now adds a new row, **afreen**'s history will look like:

```console tesh-session="history"
$ echo '{"species": "BAR"}' | RLTBL_USER=afreen rltbl --input JSON add row penguin
$ RLTBL_USER=afreen rltbl history
Insert 1 row into "penguin" table
▲ 1 change can be undone
```

**mike**'s history will be unchanged from before:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl history
▼ 4 changes can be redone
Insert 1 row into "penguin" table
Delete 1 row from "penguin" table
Update 1 row in "penguin" table
Move 1 row in "penguin" table
```
