// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {DuolingoPow} from "../src/DuolingoPow.sol";
import {TokenIncentive} from "../src/TokenIncentive.sol";

contract DuolingoPowScript is Script {
    DuolingoPow public duo;
    TokenIncentive public tokenIncentive;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        duo = new DuolingoPow();
        tokenIncentive = new TokenIncentive(address(duo));

        vm.stopBroadcast();
    }
}
