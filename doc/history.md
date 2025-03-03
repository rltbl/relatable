# History

TODO: Add some introductory documentation here.

```console tesh-session="history"
$ rltbl demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl delete row penguin 6
$ rltbl set value penguin 4 island Enderby
$ rltbl move row penguin 1 8
$ rltbl undo # Undo move row
$ rltbl undo # Undo set value
$ rltbl undo # Undo delete row
$ rltbl undo # Undo add row
$ rltbl get table penguin
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
$ rltbl history
History {
    changes_done_stack: [],
    changes_undone_stack: [
        {"change_id": "10", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":11,\"after\":10}]"},
        {"change_id": "9", "user": "mike", "table": "penguin", "description": "Add one row", "action": "undo", "content": "[{\"type\":\"Add\",\"row\":6,\"after\":5}]"},
        {"change_id": "8", "user": "mike", "table": "penguin", "description": "mike", "action": "undo", "content": "[{\"type\":\"Update\",\"row\":4,\"column\":\"island\",\"before\":\"Enderby\",\"after\":\"Torgersen\"}]"},
        {"change_id": "7", "user": "mike", "table": "penguin", "description": "Move one row", "action": "undo", "content": "[{\"type\":\"Move\",\"row\":1,\"from_after\":8,\"to_after\":0}]"},
    ],
}
```

TODO: Add more text here.

```console tesh-session="history"
$ rltbl demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl redo
$ rltbl undo
$ rltbl undo
$ rltbl undo
$ rltbl get table penguin
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
$ rltbl history
History {
    changes_done_stack: [],
    changes_undone_stack: [
        {"change_id": "10", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":11,\"after\":10}]"},
        {"change_id": "9", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":12,\"after\":11}]"},
        {"change_id": "8", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":13,\"after\":12}]"},
    ],
}
```

TODO: Add more text here.

```console tesh-session="history"
$ rltbl demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ echo '{"species": "BAR"}' | rltbl --input JSON add row penguin
$ echo '{"species": "KEW"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 12 1
$ rltbl undo
$ rltbl undo
$ rltbl undo
$ rltbl get table penguin
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
$ rltbl history
History {
    changes_done_stack: [],
    changes_undone_stack: [
        {"change_id": "10", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":11,\"after\":10}]"},
        {"change_id": "9", "user": "mike", "table": "penguin", "description": "Delete one row", "action": "undo", "content": "[{\"type\":\"Delete\",\"row\":12,\"after\":11}]"},
    ],
}
```

TODO: Add more text here.

```console tesh-session="history"
$ rltbl demo --size 10 --force
$ echo '{"species": "FOO"}' | rltbl --input JSON add row penguin
$ rltbl undo
$ rltbl move row penguin 4 9
$ rltbl undo
$ rltbl redo
$ rltbl move row penguin 3 1
$ rltbl move row penguin 4 2
$ rltbl undo
$ rltbl undo
$ rltbl undo
$ rltbl get table penguin
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
$ rltbl history
History {
    changes_done_stack: [],
    changes_undone_stack: [
        {"change_id": "10", "user": "mike", "table": "penguin", "description": "Move one row", "action": "undo", "content": "[{\"type\":\"Move\",\"row\":4,\"from_after\":9,\"to_after\":3}]"},
        {"change_id": "9", "user": "mike", "table": "penguin", "description": "Move one row", "action": "undo", "content": "[{\"type\":\"Move\",\"row\":3,\"from_after\":1,\"to_after\":2}]"},
    ],
}
```
