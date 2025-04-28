```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ rltbl -v demo --force --size 10 | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl_test -v joined-query penguin individual_id egg N1
to_sql (joined_select)): SELECT *
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "individual_id"
  FROM "penguin"
  LEFT JOIN "egg" USING ("individual_id")
  WHERE "egg"."individual_id" = ...
)
ORDER BY "penguin"._order ASC [String("N1")]
...
to_sql_count (joined_select)): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "individual_id"
  FROM "penguin"
  LEFT JOIN "egg" USING ("individual_id")
  WHERE "egg"."individual_id" = ...
) [String("N1")]
...
count (joined_select): 1

```
