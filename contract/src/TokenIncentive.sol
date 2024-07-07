// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import "contracts/base/ERC20Base.sol";
import "contracts/extension/Permissions.sol";
import "./IIncentive.sol";
import "./DuolingoPow.sol";

contract TokenIncentive is IIncentive, ERC20Base {
    DuolingoPow public pow;
    uint256 public incentiveId;

    constructor(
        address _pow
    ) ERC20Base(msg.sender, "Proof of Duolingo", "POD") {
        pow = DuolingoPow(_pow);

        incentiveId = pow.createIncentive("XP Reward: 100 POD", 10, 1000);
    }

    function _canMint() internal view override returns (bool) {
        return msg.sender == address(pow);
    }

    function claim(uint256 _incentiveId, address _receiver) external override {
        require(incentiveId == _incentiveId, "Unknown incentive ID");

        // calls _canMint
        mintTo(_receiver, 100);
    }
}
