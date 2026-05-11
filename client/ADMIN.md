# Some notes on admin

To list columns
```
csvcut -n luma.csv
```

To select colums (don't think it works if you overwrite file directly)
```
csvcut --c 2,5,23,24 > luma2.csv
```

```
csvjoin luma.csv users.csv > combined.csv
csvcut
``
