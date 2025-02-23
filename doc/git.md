# Git

Relatable can be configured to commit edits to git.
Set the `RLTBL_GIT_AUTHOR` environment variable
to the git author string to use in the commit,
e.g. "James A. Overton <james@overton.ca>".
When `RLTBL_GIT_AUTHOR` is set,
any change that modifies a table tracked by Relatable
will cause Relatable to save
all the tables with a 'path' in the 'table' table,
and then make a git commit with that author.

First we set up a git repository:

```console tesh-session="git"
$ git init
...
$ git config user.name "Alice"
$ git config user.email "alice@example.com"
$ git config core.pager cat
```

Now we set up Relatable and make a first commit:

```console tesh-session="git"
$ rltbl demo --size 10
$ rltbl save
$ echo '.relatable/' > .gitignore
$ git add .gitignore penguin.tsv
$ git commit --message 'Initial commit'
[master (root-commit) ...] Initial commit
 2 files changed, 12 insertions(+)
 create mode 100644 .gitignore
 create mode 100644 penguin.tsv
$ git log
commit ... (HEAD -> master)
Author: Alice <alice@example.com>
Date:   ...

    Initial commit
```

Without setting `RLTBL_GIT_AUTHOR`,
changes are not committed:

```console tesh-session="git"
$ rltbl set value penguin 1 study_name FOO
$ git status
On branch master
nothing to commit, working tree clean
$ git log
commit ... (HEAD -> master)
Author: Alice <alice@example.com>
Date:   ...

    Initial commit
```

When we set `RLTBL_GIT_AUTHOR`,
tables are saved and committed with that author:

```console tesh-session="git"
$ export RLTBL_GIT_AUTHOR='Bob <bob@example.com>'
$ rltbl set value penguin 1 study_name BAR
$ git log
commit ... (HEAD -> master)
Author: Bob <bob@example.com>
Date:   ...

    commit by rltbl
...
commit ...
Author: Alice <alice@example.com>
Date:   ...

    Initial commit
```

Subsequent changes by the same author within the same day
will amend the commit,
just like using `git commit --amend`:

```console tesh-session="git"
$ rltbl set value penguin 1 study_name BAZ
$ git log
commit ... (HEAD -> master)
Author: Bob <bob@example.com>
Date:   ...

    commit by rltbl
...
commit ...
Author: Alice <alice@example.com>
Date:   ...

    Initial commit
```

When the `RLTBL_GIT_AUTHOR` is different from the current commit (perhaps amended),
then a new commit is made:

```console tesh-session="git"
$ export RLTBL_GIT_AUTHOR='Cam <cam@example.com>'
$ rltbl set value penguin 1 study_name CAM
$ git log
commit ... (HEAD -> master)
Author: Cam <cam@example.com>
Date:   ...

    commit by rltbl
...
commit ...
Author: Bob <bob@example.com>
Date:   ...

    commit by rltbl
...
commit ...
Author: Alice <alice@example.com>
Date:   ...

    Initial commit
```
