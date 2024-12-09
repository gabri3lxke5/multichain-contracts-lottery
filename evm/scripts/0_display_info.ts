import {ethers} from "hardhat";
import {contractAddress} from "./common";

async function main() {

  const [owner, attestor] = await ethers.getSigners();
  console.log("owner: %s", owner.address);
  console.log("attestor: %s", attestor.address);
  console.log("Contract's address: %s", contractAddress);

  const lottoInstance = await ethers.getContractAt("RaffleRegistration", contractAddress);

  console.log("Status: %s", await lottoInstance.getStatus());
  console.log("Draw number: %s", await lottoInstance.getDrawNumber());

}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});


