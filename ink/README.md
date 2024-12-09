# Lotto multichain - Ink! Smart Contracts 

## Help

```shell
cargo help
cargo contract help
```

## Build The Lotto Manager contract

```shell
cd ./contracts/raffle_manager
cargo contract build
```

## Build The Participation contract

```shell
cd ./contracts/raffle_registration
cargo contract build
```

## Unit tests

```shell
cargo test
```

## Integration tests

```shell
cd ./integration_tests
cargo test --features=e2e-tests
```