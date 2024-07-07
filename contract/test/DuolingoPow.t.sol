// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {DuolingoPow} from "../src/DuolingoPow.sol";
import {TokenIncentive} from "../src/TokenIncentive.sol";

contract DuolingoPowTest is Test {
    DuolingoPow public duo;

    function setUp() public {
        duo = new DuolingoPow();
    }

    function test_createIncentive() public {
        uint256 incentiveId1 = duo.createIncentive("test", 10, 1);
        uint256 incentiveId2 = duo.createIncentive("test 2", 10, 1);

        assertNotEq(incentiveId1, 0);
        assertNotEq(incentiveId2, 0);
        assertNotEq(incentiveId1, incentiveId2);
    }

    function test_userActions() public {
        uint256 uid = 1928373645;
        duo.userRegister(uid, address(1), 100);

        // should start with 0 xp
        assertEq(duo.xpBalance(address(1)), 0);

        // woo! earned 20 xp
        duo.reportXp(uid, 120);

        assertEq(duo.xpBalance(address(1)), 20);
    }

    function test_claimIncentive() public {
        uint256 uid = 1928373645;
        address uaddr = address(1);
        duo.userRegister(uid, uaddr, 100);
        duo.reportXp(uid, 120);

        TokenIncentive t = new TokenIncentive(address(duo));

        uint256 incentiveId = t.incentiveId();
        uint256 startRemaining = duo.remainingClaims(incentiveId);

        vm.prank(uaddr);
        duo.claimIncentive(incentiveId);

        assertEq(duo.xpBalance(uaddr), 10);
        assertEq(t.balanceOf(uaddr), 100);
        assertEq(duo.remainingClaims(incentiveId), startRemaining - 1);
    }
}
