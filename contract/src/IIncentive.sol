// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

interface IIncentive {
    function claim(
        uint256 incentiveId,
        address receiver
    ) external;
}
