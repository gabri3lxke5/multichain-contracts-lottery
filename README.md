# Lotto Multichain dApp

## Summary

The Lotto dApp is very similar to national lotteries. The user chooses numbers and if the numbers match the winning numbers, he wins the jackpot. If there is no winner, the jackpot is put back into play.
We believe that the blockchain is really suited for this use case. All data is registered in the blockchain and can be verified. This is the Web3 version of the lotto.

Currently, the version V1, is live on Astar Network since June 14th 2024. [Lotto dApp V1](https://lucky.substrate.fi/lotto/astar#participate)

A version of the dApp V2, multichain, is deployed on the testnets since November 20th 2024: [Lotto dApp V2 - testnet](https://reorg--lotto-evm.netlify.app/)
The multichain version can be deployed on any EVM chains or Substrate chains with the Contracts pallet.
Players can participate in the lottery on different chains using either EVM or Substrate wallets, competing for the same jackpot in a single draw. The draw and jackpot are common across all chains where the Lotto multichain dApp is deployed.

Everything is automated and decentralized via WASM smart contracts or Solidity smart contracts. These smart contracts communicate with each other.

You can find a presentation of the dApp here: [Lotto dApp Presentation](https://youtu.be/r3iTKy5NOg4)

Another video explains the interaction between smart contracts: [Smart Contracts Interaction](https://youtu.be/jwdbL1Mynw8)

## Project Overview

Lotto is a multi-chain dApp that includes multiple smart contracts:

- `The Manager`:
  This [smart contract](ink/contracts/raffle_manager/lib.rs), written in Rust and Ink!, is deployed on Astar Network. It knows the state of all other smart contracts and decides what actions to perform for each smart contract. It is the brain of the application.
- `The Communicator`:
  This [smart contract](ink/contracts/raffle_registration/lib.rs), written in Rust and Ink!, is deployed on Phala Network. It enables communication between smart contracts by communicating with the contract manager and transmitting actions to other contracts.
- `The Participation Recorder` (WASM and EVM versions):
  These smart contracts record users' participation in the lottery.
  A version has been written in [Rust and Ink!](ink/contracts/raffle_registration/lib.rs)  and can be deployed on any Substrate chain where the Contracts pallet is deployed, like Astar Network.
  Another version has been written in [Solidity](evm/contracts/RaffleRegistration.sol) and can be deployed on any EVM-compatible chain like Moonbeam or any Layer 2 like Soneium.

All contracts are battle-tested via unit tests and integration tests.

### Additional Components

- `Subquery Multi-chain Indexer`:
  All data is registered on the blockchain and indexed via a Subquery Multi-chain indexer.
- `User Interface`:
  The user participates in the lottery through an intuitive graphical interface.
- `CLI Tool`:
  A [command-line interface](cli/src/lottoCli.ts) tool for deploying and configuring smart contracts and testing the whole scenario.
- `Contract Monitor`:
  A graphical interface that allows visualization of each contract's status during contract synchronization.

## Parameters of the dApp

The dApp settings should be chosen based on the size of the jackpot.
With a big jackpot, the probability of finding the winning numbers should be low.

With $10,000 in jackpot, we suggest the following parameters:
- The players can choose 5 numbers between 1 and 50. There are more 2M possibilities.
- The draw will take place draw every 100 000 blocks (around 1 week with 1 block per 6 second).
- With these parameters, the tx would have to cost less than $0.0047 for it to be economically interesting to spam the blockchains with all available combinations to have a possible gain.

With $100,000 in jackpot, we suggest the following parameters:
- The players can choose 6 numbers between 1 and 50. There are more 15M possibilities.
- The draw will take place draw every 100 000 blocks (around 1 week with 1 block per 6 second). 
- With these parameters, the tx would have to cost less than $0.0063 for it to be economically interesting to spam the blockchains with all available combinations to have a possible gain.

With $1M in jackpot, we suggest the following parameters:
- The players can choose 6 numbers between 1 and 99. There are more 1 billion possibilities.
- The draw will take place draw every 100 000 blocks (around 1 week with 1 block per 6 second).
- With these parameters, the tx would have to cost less than $0.00089 for it to be economically interesting to spam the blockchains with all available combinations to have a possible gain.

Other possible combinations:

| Parameters                 | Possible combinations |
|----------------------------|----------------------:|
| 5 numbers between 1 and 50 |              2,18,760 |
| 6 numbers between 1 and 50 |            15,890,700 |
| 7 numbers between 1 and 50 |            99,884,400 |
| 8 numbers between 1 and 50 |           536,878,650 |
| 5 numbers between 1 and 99 |            71,523,144 |
| 6 numbers between 1 and 99 |         1,120,529,256 |
| 7 numbers between 1 and 99 |        14,887,031,544 |
| 8 numbers between 1 and 99 |       171,200,862,756 |


## Where the dApp will be deployed

The Ink! smart contracts that manages the lottery must be deployed on Astar and Phala Networks.

However, the contracts that the user interacts to register numbers can be deployed on any EVM chains or Substrate chains with the Contracts pallet.

We suggest to deploy the contracts on Astar, Moonbeam and Soneium.

On Astar Network, the user will be able to play with a Substrate wallet.

On Moonbeam, the user will be able to play with a EVM wallet.

Soneium, the new Layer 2 built by Astar Team and backed by Sony, will be connected to the Superchain. This way the dApp will the first cross-chain dApp between Polkadot and Superchain.

Another important and technical information, in order to make the use of Phala's VRF (Verifiable Random Function) as secure as possible, transaction hashes are used to generate the salt provided to the VRF function.
These transaction hashes are provided by the blockchains where the dApp is deployed.

Even if some hackers found a way to hack the Verifiable Random Function provided by Phala, they would also have to find a way to control the hashes generated by all the blockchains, making the attack much more complicated.
That’s why I suggest deploying the dApp on at least three different blockchains. Of course, we can also deploy these dApps on more blockchains.


## Possible evolution with NFTs integration

NFTs can be integrated to serve as lottery tickets and NFT participation receipts.