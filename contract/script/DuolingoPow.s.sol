// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {DuolingoPow} from "../src/DuolingoPow.sol";

contract DuolingoPowScript is Script {
    DuolingoPow public duo;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        duo = new DuolingoPow("Proof of Duolingo", "POD");

        vm.stopBroadcast();
    }
}
