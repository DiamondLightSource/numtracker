# Data Directory and Filepath Lookup

Service to provide consistent file numbering and naming for unrelated data
acquisition applications.

## Running Locally

The service is written in rust and requires the default toolchain and recent
version of the compiler (1.81+). This is available from [rustup.rs][_rustup].

[_rustup]:https://rustup.rs

1. Clone this repository

    ```
    $ git clone git@github.com:DiamondLightSource/numtracker.git
    Cloning into 'numtracker'...
    ...
    $ cd numtracker
    ```

2. Build the project

    ```
    $ cargo build
    Compiling numtracker v0.1.0 (./path/to/numtracker)
    ...
    Finished 'dev' profile [unoptimized + debuginfo] target(s) in 11.56s
    ```
3. Run the service

    ```
    $ cargo run serve
    2024-11-04T11:29:05.887214Z  INFO connect{filename="numtracker.db"}: numtracker::db_service: Connecting to SQLite DB
    ```

At this point the service is running and can be queried via the graphQL
endpoints (see [the graphiql][_graphiql] front-end available at
`localhost:8000/graphiql` by default) but there are no instruments configured.

Additional logging output is available via `-v` verbose flags.

|Flags   |Level|
|--------|-----|
| `-q`   |None |
|        |Error|
| `-v`   |Info |
| `-vv`  |Debug|
| `-vvv` |Trace|

## Schema

The schema is available via the `schema` command. This is also available via the
graphiql interface.
```bash
cargo run schema
```

> [!NOTE]
> Within the schema, 'instrument' can be thought of as equivalent to 'beamline'
> in most other contexts. It is used to include other facilities such as lab
> sources and electron-microscopes.
>
> In a similar way, 'instrumentSession' is what would usually be considered a
> 'visit', i.e. a single block of time on an instrument for a proposal
> designated by a code such as cm12345-6.

## Queries

<details>
<summary markdown="span">Testing queries from terminal</summary>

While the graphiql front-end can be useful for exploring the API schema, running
from the terminal is sometimes quicker/easier. This only requires `curl`
although <a href="https://jqlang.github.io/jq/">jq</a> can make it
easier to parse output.

The query to run should be made as a POST request to `/graphql` wrapped in a
JSON object as `{"query": "<query-string>"}` taking care to escape quotes as
required. Using `curl` and a basic data directory query (see below), this
looks something like
```bash
echo '{
     "query": "{
         paths(instrument: \"i22\", instrumentSession: \"cm37278-5\") {
             path
         }
     }"
 }'| curl -s -X POST 127.0.0.1:8000/graphql -H "Content-Type: application/json" -d @- | jq
```

</details>

### Queries (read-only)
There are three read only queries, one to get the data directory for a given
instrument session and instrument, one to get the current configuration for a given
instrument and one to get the current configuration(s) for one or more
instruments.

#### paths
Get the data directory for an instrument and instrument session

##### Query
```graphql
{
  paths(instrument: "i22", instrumentSession: "cm12345-6") {
    path
    instrumentSession
  }
}
```
##### Response
```json
{
  "paths": {
    "path": "/data/i22/data/2024/cm37278-5",
    "instrumentSession": "cm37278-5"
  }
}
```

#### configuration
Get the current configuration values for the given instrument

##### Query
```graphql
{
  configuration(instrument: "i22") {
    directoryTemplate
    scanTemplate
    detectorTemplate
    dbScanNumber
    fileScanNumber
    trackerFileExtension
  }
}
```

##### Response
```json
{
  "configuration": {
    "directoryTemplate": "/data/{instrument}/data/{year}/{visit}",
    "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
    "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
    "dbScanNumber": 0,
    "fileScanNumber": null,
    "trackerFileExtension": null
  }
}
```

#### configurations
Get the current configuration values for one or more instruments specified as a
list. Providing no list returns all current configurations whereas providing an
empty list will return no configurations.

##### Query
```graphql
{
  configurations(instrumentFilters: ["i22", "i11"]) {
    instrument
    directoryTemplate
    scanTemplate
    detectorTemplate
    dbScanNumber
    fileScanNumber
    trackerFileExtension
  }
}
```

##### Response
```json
{
  "configurations": [
      {
        "instrument": "i11",
        "directoryTemplate": "/tmp/{instrument}/data/{year}/{visit}",
        "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
        "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
        "dbScanNumber": 0,
        "fileScanNumber": null,
        "trackerFileExtension": null
      },
      {
        "instrument": "i22",
        "directoryTemplate": "/tmp/{instrument}/data/{year}/{visit}",
        "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
        "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
        "dbScanNumber": 0,
        "fileScanNumber": null,
        "trackerFileExtension": null
      }
    ]
}
```

##### Query
```graphql
{
  configurations {
    instrument
    directoryTemplate
    scanTemplate
    detectorTemplate
    dbScanNumber
    fileScanNumber
    trackerFileExtension
  }
}
```

##### Response
```json
{
  "configurations": [
      {
        "instrument": "i11",
        "directoryTemplate": "/tmp/{instrument}/data/{year}/{visit}",
        "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
        "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
        "dbScanNumber": 0,
        "fileScanNumber": null,
        "trackerFileExtension": null
      },
      ...
    ]
}
```

## Mutations (read-write)

#### scan

##### Query

```graphql
mutation {
  scan(instrument: "i22", instrumentSession: "cm12345-2", subdirectory: "sub/tree") {
      scanFile
      scanNumber
      detectors(names: ["det1", "det2"] ) {
          name
          path
      }
  }
}
```

##### Response
```json
{
  "scan": {
    "scanFile": "sub/tree/i22-20840",
    "scanNumber": 20840,
    "detectors": [
      {
        "name": "det1",
        "path": "sub/tree/i22-20840-det1"
      },
      {
        "name": "det2",
        "path": "sub/tree/i22-20840-det2"
      }
    ]
  }
}
```

#### configure
##### Query
```graphql
mutation {
  configure(instrument: "i11", config: {
      directory:"/tmp/{instrument}/data/{year}/{visit}"
      scan:"{subdirectory}/{instrument}-{scan_number}"
      detector:"{subdirectory}/{instrument}-{scan_number}-{detector}"
      scanNumber: 12345
    }) {
      directoryTemplate
      scanTemplate
      detectorTemplate
      latestScanNumber
    }
  }
}
```
##### Response
```json
{
  "configure": {
    "directoryTemplate": "/tmp/{instrument}/data/{year}/{visit}",
    "scanTemplate": "{subdirectory}/{instrument}-{scan_number}",
    "detectorTemplate": "{subdirectory}/{instrument}-{scan_number}-{detector}",
    "latestScanNumber": 12345
  }
}
```

[_graphiql]:https://github.com/graphql/graphiql/
[_jq]:https://jqlang.github.io/jq/
