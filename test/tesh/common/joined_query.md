```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ rltbl -v demo --size 10 --force | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl_test -vvv joined-query penguin individual_id egg N1
... DEBUG rltbl_test: CLI Cli { database: ".relatable/relatable.db", user: "", verbose: Verbosity { verbose: 3, quiet: 0, phantom: PhantomData<clap_verbosity_flag::ErrorLevel> }, vertical: false, seed: None, command: JoinedQuery { table1: "penguin", column: "individual_id", table2: "egg", value: String("N1") } }
... INFO rltbl_test: SELECT: Select { table_name: "penguin", view_name: "", select: [], joins: [], limit: 0, offset: 0, filters: [Equal { table: "egg", column: "individual_id", value: String("N1") }], order_by: [] }
... INFO rltbl_test: SELECT (JOINED): Select { table_name: "penguin", view_name: "", select: [], joins: [], limit: 0, offset: 0, filters: [InSubquery { table: "", column: "individual_id", subquery: Select { table_name: "penguin", view_name: "", select: ["individual_id"], joins: ["LEFT JOIN \"egg\" USING (\"individual_id\")"], limit: 0, offset: 0, filters: [Equal { table: "egg", column: "individual_id", value: String("N1") }], order_by: [] } }], order_by: [] }
... DEBUG rltbl::core: SQL (COUNT) SELECT COUNT(*) AS "count"
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "individual_id"
  FROM "penguin"
  LEFT JOIN "egg" USING ("individual_id")
  WHERE "egg"."individual_id" = ?
)
... INFO rltbl_test: COUNT: 1
```
