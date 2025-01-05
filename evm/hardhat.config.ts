import { HardhatUserConfig, vars, task } from "hardhat/config";
import "@nomicfoundation/hardhat-toolbox";

const MINATO_OWNER_PRIVATE_KEY = vars.get("LOTTO_MINATO_OWNER_KEY");
const MINATO_EVM_ATTESTOR_PK = vars.get("LOTTO_MINATO_ATTESTOR_KEY");
const MINATO_USER1_PRIVATE_KEY = vars.get("LOTTO_MINATO_USER1_KEY");
const MINATO_USER2_PRIVATE_KEY = vars.get("LOTTO_MINATO_USER2_KEY");
const MOONBASE_API_KEY = vars.get("MOONBASE_API_KEY");

task("accounts", "Prints the list of accounts", async (_taskArgs, hre) => {
  const accounts = await hre.ethers.getSigners();

  for (const account of accounts) {
    console.log(account.address);
  }
});

task("balance", "Prints an account's balance", async (taskArgs, hre) => {
  // @ts-ignore
  const balance = await hre.ethers.provider.getBalance(taskArgs.account);
  console.log(hre.ethers.formatEther(balance), "ETH");
}).addParam("account", "The account's address");

task("balances", "Prints the balance for all accounts", async (_taskArgs, hre) => {

  const accounts = await hre.ethers.getSigners();

  for (const account of accounts) {
    const balance =  await hre.ethers.provider.getBalance(account.address);
    const formattedBalance = hre.ethers.formatEther(balance);
    console.log("%s - %s ETH", account.address, formattedBalance);
  }

});

task("cancel", "Cancel pending transactions", async (_taskArgs, hre) => {
  const accounts = await hre.ethers.getSigners();

  const owner = accounts[0];
  const tx = {
    nonce: 107,
    to: owner.address,
    value: 0,
    //gaslimit: 58000;
    gasPrice: hre.ethers.parseUnits('50', 'gwei')
  }
  await owner.sendTransaction(tx);

});


const config: HardhatUserConfig = {
  solidity: {
    version:  "0.8.24",
  },
  ignition: {
    disableFeeBumping: false,
    maxFeeBumps: 10,
    timeBeforeBumpingFees: 1 * 60 * 1_000, // 1 minutes
    requiredConfirmations: 5,
  },
  networks: {
    'minato': {
      url: 'https://rpc.minato.soneium.org',
      accounts: [MINATO_OWNER_PRIVATE_KEY, MINATO_EVM_ATTESTOR_PK, MINATO_USER1_PRIVATE_KEY, MINATO_USER2_PRIVATE_KEY],
      /*
      ignition: {
        maxFeePerGasLimit: 500_000_000_000n, // 500 gwei
        maxPriorityFeePerGas: 2_000_000_000n, // 2 gwei
        disableFeeBumping: false,
      }
       */
    },
    'moonbase': {
      url: 'https://rpc.api.moonbase.moonbeam.network',
      accounts: [MINATO_OWNER_PRIVATE_KEY, MINATO_EVM_ATTESTOR_PK, MINATO_USER1_PRIVATE_KEY, MINATO_USER2_PRIVATE_KEY]
    },
    'shibuya': {
      url: 'https://evm.shibuya.astar.network',
      accounts: [MINATO_OWNER_PRIVATE_KEY, MINATO_EVM_ATTESTOR_PK, MINATO_USER1_PRIVATE_KEY, MINATO_USER2_PRIVATE_KEY]
    },
    'base-sepolia': {
      url: 'https://sepolia.base.org',
      accounts: [MINATO_OWNER_PRIVATE_KEY, MINATO_EVM_ATTESTOR_PK, MINATO_USER1_PRIVATE_KEY, MINATO_USER2_PRIVATE_KEY]
    },
  },

  etherscan: {
    apiKey: {
      'minato': 'empty',
      'moonbase': MOONBASE_API_KEY,
      'shibuya': 'empty',
      'base-sepolia': 'empty'
    },
    customChains: [
      {
        network: "minato",
        chainId: 1946,
        urls: {
          apiURL: "https://soneium-minato.blockscout.com/api",
          browserURL: "https://soneium-minato.blockscout.com"
        }
      },
      {
        network: "moonbase",
        chainId: 1287,
        urls: {
          apiURL: "https://api-moonbase.moonscan.io/api",
          browserURL: "https://moonbase.moonscan.io"
        }
      },
      {
        network: "shibuya",
        chainId: 81,
        urls: {
          apiURL: "https://shibuya.blockscout.com/api",
          browserURL: "https://shibuya.blockscout.com",
        }
      },
      {
        network: "base-sepolia",
        chainId: 84532,
        urls: {
          apiURL: "https://eth-sepolia.blockscout.com/api",
          browserURL: "https://eth-sepolia.blockscout.com"
        }
      }
    ]
  }
};

export default config;

