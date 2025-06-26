# Editing your data

To add rows to a table on the command line, one uses `rltbl add row TABLE`. **rltbl** normally adds rows interactively by asking the user to supply a value for every column in the table in turn. Alternatively, the option `--input JSON` may be specified to accept the row to be added in the form a JSON-formatted string.

```console tesh-session="history"
$ rltbl -v demo --size 10 --force
Created a demonstration database in '.relatable/relatable.db'
$ echo '{"species": "FOO"}' | RLTBL_USER=mike rltbl -v --input JSON add row penguin
```
Note the use of the environment variable, `RLTBL_USER`, to specify the user associated with this particular action. This can be done on a per-command basis, as we have done here, or alternately (the usual setup) one can set the environment variable in one's shell initialization script (e.g., `~/.bashrc`). Because the examples below depend sensitively on which actions are owned by which user, we have been careful to be explicit about the user of each command below for which it is relevant.

**rltbl** can undo and redo previous actions, and display the history of previous actions for this user.

```console tesh-session="history"
$ RLTBL_USER=mike rltbl -v undo
$ RLTBL_USER=mike rltbl -v redo
$ RLTBL_USER=mike rltbl -v delete row penguin 6
$ RLTBL_USER=mike rltbl -v set value penguin 4 island Enderby
$ RLTBL_USER=mike rltbl -v move row penguin 1 8
$ RLTBL_USER=mike rltbl -v undo
```

The contents of the penguin table are now:

```console tesh-session="history"
$ rltbl -v get table penguin
Rows 1-10 of 10
study_name  sample_number  species             island     individual_id  culmen_length  culmen_depth  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.60          31.10         4093
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.50          33.40         3336
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.20          22.40         4087
FAKE123     4              Pygoscelis adeliae  Enderby    N4             34.30          35.80         3469
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             40.60          39.90         2129
FAKE123     7              Pygoscelis adeliae  Biscoe     N7             38.60          28.50         3607
FAKE123     8              Pygoscelis adeliae  Dream      N8             33.80          39.90         1908
FAKE123     9              Pygoscelis adeliae  Dream      N9             43.70          23.10         3883
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
                           FOO
```

We use the **history** subcommand to get information about the last few commands that can be undone or redone.
The line prefixed with a down-arrow, as well as those below it, can be undone. The other lines represent
undone changes that can  be redone.

```console tesh-session="history"
$ RLTBL_USER=mike rltbl -v history
▲ Move row 1 from after row 8 to after row 0 (action #7, undo)
▼ Update 'island' in row 4 from Biscoe to Enderby (action #5, do)
  Delete row 6 (action #4, do)
  Add row 11 after row 10 (action #3, redo)
```

To restore the original state of the table we can finally do:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl -v undo # Undo set value
$ RLTBL_USER=mike rltbl -v undo # Undo delete row
$ RLTBL_USER=mike rltbl -v undo # Undo add row
$ RLTBL_USER=mike rltbl -v get table penguin
Rows 1-10 of 10
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
FAKE123     10             Pygoscelis adeliae  Torgersen  N10            31.50          30.00         4521
```

As already mentioned, **rltbl** supports multiple users. It also supports and keeps track of multiple user histories. Although **mike**'s history currently looks like the following:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl -v history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

The same is not true of **afreen**, who has not yet performed any actions:

```console tesh-session="history"
$ RLTBL_USER=afreen rltbl -v history

```

If she now adds a new row, **afreen**'s history will look like:

```console tesh-session="history"
$ echo '{"species": "BAR"}' | RLTBL_USER=afreen rltbl -v --input JSON add row penguin
$ RLTBL_USER=afreen rltbl -v history
▼ Add row 12 after row 10 (action #11, do)
```

**mike**'s history will be unchanged from before:

```console tesh-session="history"
$ RLTBL_USER=mike rltbl -v history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'island' in row 4 from Enderby to Biscoe (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```
