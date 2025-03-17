# History

To add rows to a table on the command line, one uses `rltbl add row TABLE`. *rltbl* normally adds rows interactively by asking the user to supply a value for every column in the table in turn. Alternatively, the option `--input JSON` may be specified to accept the row to be added in the form a JSON-formatted string. For instance:

```console tesh-session="history"
$ rltbl -v demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl -v --input JSON add row penguin
```

*rltbl* can undo and redo previous actions, and display the history of previous actions for this user.

```console tesh-session="history"
$ rltbl -v undo
$ rltbl -v redo
$ rltbl -v delete row penguin 6
$ rltbl -v set value penguin 4 island Enderby
$ rltbl -v move row penguin 1 8
$ rltbl -v undo
```

The contents of the penguin table are now:

```console tesh-session="history"
$ rltbl -v get table penguin
Rows 1-10 of 10
study_name  sample_number  species             island     individual_id  culmen_length  body_mass
FAKE123     1              Pygoscelis adeliae  Torgersen  N1             44.6           3221
FAKE123     2              Pygoscelis adeliae  Torgersen  N2             30.5           3685
FAKE123     3              Pygoscelis adeliae  Torgersen  N3             35.2           1491
FAKE123     4              Pygoscelis adeliae  Enderby    N4             31.4           1874
FAKE123     5              Pygoscelis adeliae  Torgersen  N5             45.8           3469
FAKE123     7              Pygoscelis adeliae  Torgersen  N7             49.9           2129
FAKE123     8              Pygoscelis adeliae  Biscoe     N8             30.9           1451
FAKE123     9              Pygoscelis adeliae  Biscoe     N9             38.6           2702
FAKE123     10             Pygoscelis adeliae  Dream      N10            33.8           4697
null        null           FOO                 null       null           null           null
```

We use the *history* subcommand to get information about the last few commands that can be undone or redone.
The line prefixed with a down-arrow, as well as those below it, can be undone. The other lines represent
undone changes that can  be redone.

```console tesh-session="history"
$ rltbl -v history
▲ Move row 1 from after row 8 to after row 0 (action #7, undo)
▼ Update 'island' in row 4 from Torgersen to Enderby (action #5, do)
  Delete row 6 (action #4, do)
  Add row 11 after row 10 (action #3, redo)
```

To restore the original state of the table we can finally do:

```console tesh-session="history"
$ rltbl -v undo # Undo set value
$ rltbl -v undo # Undo delete row
$ rltbl -v undo # Undo add row
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

*rltbl* supports multiple users, and multiple user histories. The user associated with a particular command may be specified using the environment variable, `RLTBL_USER`. Although *mike*'s history (the default user on my system, which is why it was unspecified above) looks like the following:

```console tesh-session="history"
$ rltbl -v history
  Move row 1 from after row 8 to after row 0 (action #7, undo)
  Update 'island' in row 4 from Enderby to Torgersen (action #8, undo)
  Add row 6 after row 5 (action #9, undo)
▲ Delete row 11 (action #10, undo)
```

The same is not true of *afreen*, who has not yet performed any actions:

```console tesh-session="history"
$ RLTBL_USER=afreen rltbl -v history

```

If she now adds a new row, *afreen*'s history will look like:

```console tesh-session="history"
$ echo '{"species": "BAR"}' | RLTBL_USER=afreen rltbl -v --input JSON add row penguin
$ RLTBL_USER=afreen rltbl -v history
▼ Add row 12 after row 10 (action #11, do)
```

# *mike*'s history will be unchanged from before:
# 
# ```console tesh-session="history"
# $ RLTBL_USER=mike rltbl -v history
#   Move row 1 from after row 8 to after row 0 (action #7, undo)
#   Update 'island' in row 4 from Enderby to Torgersen (action #8, undo)
#   Add row 6 after row 5 (action #9, undo)
# ▲ Delete row 11 (action #10, undo)
# ```
