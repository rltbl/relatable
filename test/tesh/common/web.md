```console tesh-session="test"
$ echo "Created a demonstration database in '$RLTBL_CONNECTION'" > expected_output.txt
$ rltbl -v demo --size 10 --force | diff - expected_output.txt
$ rm -f expected_output.txt
$ rltbl serve --port 9000 --timeout 5 &
...
$ curl http://0.0.0.0:9000/table/penguin
...
<p class="range">Rows 1-10 of 10</p>
<table class="table">
  <thead>
    <tr>
...
      <th>study_name</th>
...
      <th>sample_number</th>
...
      <th>species</th>
...
      <th>island</th>
...
      <th>individual_id</th>
...
      <th>culmen_length</th>
...
      <th>body_mass</th>
...
    </tr>
  </thead>
  <tbody>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>1</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N1</td>
...
      <td>44.6</td>
...
      <td>3221</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>2</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N2</td>
...
      <td>30.5</td>
...
      <td>3685</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>3</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N3</td>
...
      <td>35.2</td>
...
      <td>1491</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>4</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N4</td>
...
      <td>31.4</td>
...
      <td>1874</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>5</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N5</td>
...
      <td>45.8</td>
...
      <td>3469</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>6</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N6</td>
...
      <td>40.6</td>
...
      <td>4875</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>7</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Torgersen</td>
...
      <td>N7</td>
...
      <td>49.9</td>
...
      <td>2129</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>8</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Biscoe</td>
...
      <td>N8</td>
...
      <td>30.9</td>
...
      <td>1451</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>9</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Biscoe</td>
...
      <td>N9</td>
...
      <td>38.6</td>
...
      <td>2702</td>
...
    </tr>
...
    <tr>
...
      <td>FAKE123</td>
...
      <td>10</td>
...
      <td>Pygoscelis adeliae</td>
...
      <td>Dream</td>
...
      <td>N10</td>
...
      <td>33.8</td>
...
      <td>4697</td>
...
    </tr>
...
  </tbody>
</table>
...
$ wait
...
```
