// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import "contracts/extension/Ownable.sol";
import "./IIncentive.sol";

struct Incentive {
    IIncentive target;
    string name;
    uint256 requiredXp;
    uint256 remaining;
}

struct User {
    address addr;
    uint256 xp;
    uint256 xpBalance;
}

contract DuolingoPow is Ownable {
    mapping(uint256 => User) public users;
    mapping(address => uint256) public addressToUid;
    mapping(uint256 => Incentive) public incentives;
    mapping(bytes32 => bool) public claimedIncentives;

    uint256 internal nextIncentiveId = 1;

    event UserRegistrationUpdate(
        uint256 indexed _uid,
        address indexed _address
    );
    event UserXpUpdate(uint256 indexed _uid, uint256 _xp);
    event IncentiveClaim(
        uint256 indexed _incentiveId,
        uint256 indexed _uid,
        address indexed _address
    );

    function userRegister(
        uint256 _uid,
        address _address,
        uint256 _xp
    ) external onlyOwner {
        require(_address != address(0), "Invalid address");

        require(users[_uid].addr == address(0), "UID is already registered");
        require(addressToUid[_address] == 0, "Address is already registered");

        // don't count xp that was earned before registration
        users[_uid] = User(_address, _xp, 0);
        addressToUid[_address] = _uid;

        emit UserRegistrationUpdate(_uid, _address);
    }

    modifier requireRegisteredUid(uint256 _uid) {
        require(users[_uid].addr != address(0), "UID is not registered");

        _;
    }

    modifier requireRegisteredAddress(address _address) {
        require(addressToUid[_address] != 0, "Address is not registered");

        _;
    }

    function userUnregister(
        uint256 _uid
    ) external onlyOwner requireRegisteredUid(_uid) {
        address _address = users[_uid].addr;
        delete users[_uid];
        delete addressToUid[_address];

        emit UserRegistrationUpdate(_uid, address(0));
    }

    function userUpdateAddress(
        uint256 _uid,
        address _address
    ) external onlyOwner requireRegisteredUid(_uid) {
        require(_address != address(0), "Invalid address");

        address _oldAddress = users[_uid].addr;
        delete addressToUid[_oldAddress];
        addressToUid[_address] = _uid;
        users[_uid].addr = _address;

        emit UserRegistrationUpdate(_uid, _address);
    }

    function reportXp(
        uint256 _uid,
        uint256 _xp
    ) external onlyOwner requireRegisteredUid(_uid) {
        // TODO: this assumes that XP can only increase, which is probably not true (deleting courses?)
        require(
            _xp > users[_uid].xp,
            "Reported XP must be higher than previous XP"
        );

        unchecked {
            users[_uid].xpBalance += _xp - users[_uid].xp;
        }
        users[_uid].xp = _xp;

        emit UserXpUpdate(_uid, _xp);
    }

    function _newIncentiveId() internal returns (uint256) {
        uint256 _id = nextIncentiveId;
        nextIncentiveId++;
        return _id;
    }

    function createIncentive(
        string calldata _name,
        uint256 _xpRequirement,
        uint256 _limit
    ) external returns (uint256) {
        require(_limit > 0, "Invalid limit");
        require(_xpRequirement > 0, "Invalid XP requirement");

        uint256 _incentiveId = _newIncentiveId();
        incentives[_incentiveId] = Incentive(
            IIncentive(msg.sender),
            _name,
            _xpRequirement,
            _limit
        );

        return _incentiveId;
    }

    function incentiveClaimKey(
        uint256 _uid,
        uint256 _incentiveId
    ) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(_uid, _incentiveId));
    }

    function claimIncentive(
        uint256 _incentiveId
    ) external requireRegisteredAddress(msg.sender) {
        uint256 _uid = addressToUid[msg.sender];
        bytes32 _key = incentiveClaimKey(_uid, _incentiveId);
        require(!claimedIncentives[_key], "Incentive has already been claimed");
        uint256 _requiredXp = incentives[_incentiveId].requiredXp;
        require(
            users[_uid].xpBalance >= _requiredXp,
            "Insufficient XP balance"
        );
        require(
            incentives[_incentiveId].remaining > 0,
            "Incentive has already been claimed"
        );
        users[_uid].xpBalance -= _requiredXp;
        incentives[_incentiveId].remaining--;
        claimedIncentives[_key] = true;

        IIncentive(incentives[_incentiveId].target).claim(
            _incentiveId,
            msg.sender
        );

        emit IncentiveClaim(_incentiveId, _uid, msg.sender);
    }

    function xpBalance(
        address _address
    ) external view requireRegisteredAddress(_address) returns (uint256) {
        uint256 _uid = addressToUid[_address];
        return users[_uid].xpBalance;
    }

    function remainingClaims(
        uint256 _incentiveId
    ) external view returns (uint256) {
        return incentives[_incentiveId].remaining;
    }

    constructor() {
        _setupOwner(msg.sender);
    }

    function _canSetOwner() internal view virtual override returns (bool) {
        return msg.sender == owner();
    }
}
