// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {DuolingoPow} from "../src/DuolingoPow.sol";

contract DuolingoPowTest is Test {
    DuolingoPow public duo;

    function setUp() public {
        duo = new DuolingoPow("Proof of Duolingo", "POD");
    }

    function test_userActions() public {
        uint256 uid = 1928373645;
        duo.userRegister(uid, address(1), 100);

        // should start with 0 xp
        assertEq(duo.balanceOf(address(1)), 0);

        // woo! earned 20 xp
        duo.reportXp(uid, 120);

        assertEq(duo.balanceOf(address(1)), 20 ether);
    }
}
