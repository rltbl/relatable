```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ rltbl -v demo --force --size 10 | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl_test -v select-join penguin individual_id egg N1
TO_SQL (SELECT_1): SELECT *
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "penguin"."individual_id" = ...
)
ORDER BY "penguin"._order ASC [String("N1")]
...
TO_SQL_COUNT (SELECT_1): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "penguin"."individual_id" = ...
) [String("N1")]
...
ROWS (SELECT_1): [
    Row {
        id: 1,
        order: 1000,
        change_id: 0,
        cells: {
            "study_name": Cell {
                value: String("FAKE123"),
                text: "FAKE123",
                messages: [],
            },
            "sample_number": Cell {
                value: String("1"),
                text: "1",
                messages: [],
            },
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
            "island": Cell {
                value: String("Torgersen"),
                text: "Torgersen",
                messages: [],
            },
            "individual_id": Cell {
                value: String("N1"),
                text: "N1",
                messages: [],
            },
            "culmen_length": Cell {
                value: String("44.6"),
                text: "44.6",
                messages: [],
            },
            "body_mass": Cell {
                value: String("3221"),
                text: "3221",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_1): 1
...
TO_SQL (SELECT_2): SELECT *
FROM "penguin_default_view"
WHERE "penguin_default_view"."sample_number" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("5")]
...
TO_SQL_COUNT (SELECT_2): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" = ... [String("5")]
...
ROWS (SELECT_2): [
    Row {
        id: 5,
        order: 5000,
        change_id: 0,
        cells: {
            "study_name": Cell {
                value: String("FAKE123"),
                text: "FAKE123",
                messages: [],
            },
            "sample_number": Cell {
                value: String("5"),
                text: "5",
                messages: [],
            },
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
            "island": Cell {
                value: String("Torgersen"),
                text: "Torgersen",
                messages: [],
            },
            "individual_id": Cell {
                value: String("N5"),
                text: "N5",
                messages: [],
            },
            "culmen_length": Cell {
                value: String("45.8"),
                text: "45.8",
                messages: [],
            },
            "body_mass": Cell {
                value: String("3469"),
                text: "3469",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_2): 1
...
TO_SQL (SELECT_3): SELECT
  "penguin_default_view"."species"
FROM "penguin_default_view"
WHERE "penguin_default_view"."sample_number" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("9")]
...
TO_SQL_COUNT (SELECT_3): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" = ... [String("9")]
...
ROWS (SELECT_3): [
    Row {
        id: 0,
        order: 0,
        change_id: 0,
        cells: {
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_3): 1
...
TO_SQL (SELECT_4): SELECT
  "penguin_default_view"."species",
  "penguin_default_view"."island",
  "penguin_default_view"."study_name",
  "penguin_default_view"."body_mass"
FROM "penguin_default_view"
WHERE "penguin_default_view"."island" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("Biscoe")]
...
TO_SQL_COUNT (SELECT_4): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."island" = ... [String("Biscoe")]
...
ROWS (SELECT_4): [
    Row {
        id: 0,
        order: 0,
        change_id: 0,
        cells: {
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
            "island": Cell {
                value: String("Biscoe"),
                text: "Biscoe",
                messages: [],
            },
            "study_name": Cell {
                value: String("FAKE123"),
                text: "FAKE123",
                messages: [],
            },
            "body_mass": Cell {
                value: String("1451"),
                text: "1451",
                messages: [],
            },
        },
    },
    Row {
        id: 0,
        order: 0,
        change_id: 0,
        cells: {
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
            "island": Cell {
                value: String("Biscoe"),
                text: "Biscoe",
                messages: [],
            },
            "study_name": Cell {
                value: String("FAKE123"),
                text: "FAKE123",
                messages: [],
            },
            "body_mass": Cell {
                value: String("2702"),
                text: "2702",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_4): 2
...
TO_SQL (SELECT_5): SELECT
  "penguin_default_view"."island" AS "location"
FROM "penguin_default_view"
WHERE "penguin_default_view"."sample_number" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("9")]
...
TO_SQL_COUNT (SELECT_5): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" = ... [String("9")]
...
ROWS (SELECT_5): [
    Row {
        id: 0,
        order: 0,
        change_id: 0,
        cells: {
            "location": Cell {
                value: String("Biscoe"),
                text: "Biscoe",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_5): 1
...
TO_SQL (SELECT_6): SELECT
  CASE WHEN island = 'Biscoe' THEN 'BISCOE' END AS "location"
FROM "penguin_default_view"
WHERE "penguin_default_view"."sample_number" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("9")]
...
TO_SQL_COUNT (SELECT_6): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" = ... [String("9")]
...
ROWS (SELECT_6): [
    Row {
        id: 0,
        order: 0,
        change_id: 0,
        cells: {
            "location": Cell {
                value: String("BISCOE"),
                text: "BISCOE",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_6): 1
...
TO_SQL (SELECT_7): SELECT
  "penguin_default_view"."_id",
  "penguin_default_view"."_order",
  "penguin_default_view"."study_name",
  "penguin_default_view"."sample_number",
  "penguin_default_view"."species",
  "penguin_default_view"."island",
  "penguin_default_view"."individual_id",
  "penguin_default_view"."culmen_length",
  "penguin_default_view"."body_mass"
FROM "penguin_default_view"
WHERE "penguin_default_view"."sample_number" = ...
ORDER BY "penguin_default_view"._order ASC
LIMIT 100 [String("9")]
...
TO_SQL_COUNT (SELECT_7): SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" = ... [String("9")]
...
ROWS (SELECT_7): [
    Row {
        id: 9,
        order: 9000,
        change_id: 0,
        cells: {
            "study_name": Cell {
                value: String("FAKE123"),
                text: "FAKE123",
                messages: [],
            },
            "sample_number": Cell {
                value: String("9"),
                text: "9",
                messages: [],
            },
            "species": Cell {
                value: String("Pygoscelis adeliae"),
                text: "Pygoscelis adeliae",
                messages: [],
            },
            "island": Cell {
                value: String("Biscoe"),
                text: "Biscoe",
                messages: [],
            },
            "individual_id": Cell {
                value: String("N9"),
                text: "N9",
                messages: [],
            },
            "culmen_length": Cell {
                value: String("38.6"),
                text: "38.6",
                messages: [],
            },
            "body_mass": Cell {
                value: String("2702"),
                text: "2702",
                messages: [],
            },
        },
    },
]
COUNT (SELECT_7): 1

```
