# Integration tests for Ink! Smart Contracts

Before you can run the integration tests, you have to install a Substrate node with pallet-contracts. By default, `e2e tests` require that you install `substrate-contracts-node`. You do not need to run it in the background since the node is started for each test independently. To install the latest version:
```bash
cargo install contracts-node --git https://github.com/paritytech/substrate-contracts-node.git
```

If you want to run any other node with pallet-contracts you need to change `CONTRACTS_NODE` environment variable:
```bash
export CONTRACTS_NODE="YOUR_CONTRACTS_NODE_PATH"
```

And finally execute the following command to start e2e tests execution.
```shell
cd ../../integration_tests
cargo test --features=e2e-tests
```