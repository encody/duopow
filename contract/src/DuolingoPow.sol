// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import "contracts/extension/Ownable.sol";
import "contracts/base/ERC20Base.sol";

struct User {
    address addr;
    uint256 xp;
}

contract DuolingoPow is ERC20Base {
    mapping(uint256 => User) public users;
    mapping(address => uint256) public addressToUid;

    event UserRegistrationUpdate(
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
        users[_uid] = User(_address, _xp);
        addressToUid[_address] = _uid;

        emit UserRegistrationUpdate(_uid, _address);
    }

    modifier requireRegisteredUid(uint256 _uid) {
        require(users[_uid].addr != address(0), "UID is not registered");

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

        uint256 delta;
        unchecked {
            delta = _xp - users[_uid].xp;
        }
        mintTo(users[_uid].addr, delta * 1 ether);

        users[_uid].xp = _xp;
    }

    constructor(
        string memory _name,
        string memory _symbol
    ) ERC20Base(msg.sender, _name, _symbol) {}
}
