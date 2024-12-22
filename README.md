# am2am - Alertmanager-to-Alertmanager Proxy

![build workflow](https://github.com/opsplane-services/am2am/actions/workflows/ci.yml/badge.svg)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

`am2am` is a proxy application designed to handle incoming webhooks from Alertmanager and forward them to another Alertmanager instance. It supports routing to multiple Alertmanager instances based on labels, as well as optional Basic Authentication.

## Features

- Proxy webhooks from Alertmanager to another Alertmanager instance.
- Route alerts to multiple Alertmanager instances based on label values.
- Support for Basic Authentication for Alertmanager endpoints.
- YAML-based configuration for defining multiple Alertmanager endpoints.
- Default fallback Alertmanager for unmatched or unrouted alerts.

## Usage

### Docker

```bash
docker run --rm -e RUST_LOG=trace -e ALERTMANAGER_URL=http://localhost:9093/api/v2/alerts  ghcr.io/opsplane-services/am2am:latest
```

If `ENABLE_LABEL_ROUTING` pass a `yaml` configuration (as `alertmanagers.yaml`, next to the binary, or however it is defined by `ALERTMANAGER_CONFIG` env variable)

## Configuration

The application can be configured using environment variables and a YAML configuration file.

### Environment Variables

- `ALERTMANAGER_URL`: URL of the default Alertmanager instance. Required (e.g.: `http://localhost:9093/api/v2/alerts`)
- `DEFAULT_USERNAME`: Username for Basic Authentication with the default Alertmanager.
- `DEFAULT_PASSWORD`: Password for Basic Authentication with the default Alertmanager.
- `ALERTMANAGER_CONFIG`: Path to the YAML configuration file for additional Alertmanager instances.	Default value: `alertmanagers.yaml`
- `LABEL_ROUTING_KEY`: Label key used for routing alerts to specific Alertmanager instances. Default value: `alertmanager`
- `ENABLE_LABEL_ROUTING`: Enable or disable label-based routing. Set to true to enable, otherwise only use the default Alertmanager. Default value: `true`
- `RUST_LOG`: Log level for the application. Supported values: info, debug, warn, error. Default value: info
- `SERVER_ADDRESS`: Use custom server address. Default value: `0.0.0.0:8000`

### YAML Configuration
The `ALERTMANAGER_CONFIG` YAML file defines additional Alertmanager instances and their authentication details.

Example alertmanagers.yaml:

```yaml
key1:
  url: https://alertmanager1.example.com
  auth:
    username: ENV_USERNAME1
    password: ENV_PASSWORD1

key2:
  url: https://alertmanager2.example.com
  auth:
    username: ENV_USERNAME2
    password: ENV_PASSWORD2
```
#### Fields

- root keys: Unique identifiers for each Alertmanager instance. This name should match for the label based rooting.
- `url`: URL of the Alertmanager instance.
- `auth`: Optional authentication details.
- `username`: Environment variable name for the username.
- `password`: Environment variable name for the password.

Authentication credentials are resolved using the provided environment variable names. If auth is omitted, the Alertmanager instance will not use Basic Authentication.

## How It Works

1. Incoming Webhook 

Alerts are received at the `/api/v2/alerts` endpoint.

2. Routing

If `ENABLE_LABEL_ROUTING` is true, the application checks the label specified in `LABEL_ROUTING_KEY` from the incoming alerts.Based on the label value, the corresponding Alertmanager instance from the YAML configuration is selected.
If no matching instance is found, the default Alertmanager (`ALERTMANAGER_URL`) is used.

3. Forwarding:

Alerts are forwarded as JSON to the target Alertmanager instance.
Basic Authentication is applied if configured.

4. Response

Returns a status indicating whether the alert was successfully forwarded.

## Example Usage

Environment Setup:

```bash
export ALERTMANAGER_URL=https://default-alertmanager.example.com
export DEFAULT_USERNAME=default_user
export DEFAULT_PASSWORD=default_pass
export ALERTMANAGER_CONFIG=alertmanagers.yaml
export LABEL_ROUTING_KEY=custom_label
export ENABLE_LABEL_ROUTING=true
export SERVER_ADDRESS=0.0.0.0;8080
```

Starting the Application:

```bash
cargo run --release
```

Sending Alerts:

Use curl to send alerts to the proxy:

```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"alerts": [{"labels": {"custom_label": "key1"}, "annotations": {"summary": "Test alert"}}]}' \
  http://localhost:8080/api/v2/alerts
```

## Development

### Prerequisites

- Rust (for building the application)
- Docker (optional, for containerized deployment)

### Build

```bash
cargo build --release
```
### Run

```bash
./target/release/am2am
```

### Docker

Build and run the application in a Docker container:

```bash
docker build -t am2am .
docker run -p 8080:8080 -e ALERTMANAGER_URL=<default_alertmanager_url> am2am
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

This project is licensed under the MIT License.
