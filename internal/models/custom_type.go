package models

type UserStatus uint8
type TopicStatus uint8
type TopicType uint8
type VoteStatus bool

type AdminPermission uint8

const (
	UserStatusNormal UserStatus = iota
	UserStatusDeleted
	UserStatusBanned
)

const (
	TopicStatusAudit TopicStatus = iota
	TopicStatusNormal
	TopicStatusDeleted
	TopicStatusEnded
)

const (
	// TopicTypeSixStarCharacter 六星干员
	TopicTypeSixStarCharacter TopicType = iota
	// TopicTypeAnyStarCharacter 全干员
	TopicTypeAnyStarCharacter
	// TopicTypeCollection 肉鸽藏品
	TopicTypeCollection
	// TopicTypeCustom 自定义
	TopicTypeCustom TopicType = 255
)

const (
	// AdminPermissionDefault 默认权限
	AdminPermissionDefault AdminPermission = iota
	// AdminPermissionSuper 超级管理员
	AdminPermissionSuper
)
