package models

import (
	"github.com/goccy/go-json"
	"github.com/skadiD/database/types"
)

type Topic struct {
	Id int `json:"id" db:"id" orm:"id,pk,auto"`
	// PublicId types.PublicId `json:"public_id" db:"public_id" orm:"public_id"`
	// 投票主题外显名称
	Name string `json:"name" db:"name" orm:"name"`
	// 投票主题描述
	Description string `json:"description" db:"description" orm:"description"`
	// 投票主题类型 enum
	//
	// TopicTypeSixStarCharacter TopicTypeAnyStarCharacter TopicTypeCollection TopicTypeCustom
	Type TopicType `json:"type" db:"type" orm:"type"`
	// 样式（背景图片，特殊box）
	Style json.RawMessage `json:"style" db:"style" orm:"style"`
	// 状态 iota-enum
	//
	// TopicStatusAudit TopicStatusNormal TopicStatusDeleted TopicStatusEnded
	Status TopicStatus `json:"status" db:"status" orm:"status"`
	// 投票开始时间
	StartAt types.JsonTime `json:"start_at" db:"start_at" orm:"start_at"`
	// 投票结束时间
	FinishAt types.JsonTime `json:"finish_at" db:"finish_at" orm:"finish_at"`
	// 创建时间
	CreatedAt types.JsonTime `json:"created_at" db:"created_at" orm:"created_at"`
	// 更新时间
	UpdatedAt types.JsonTime `json:"updated_at" db:"updated_at" orm:"updated_at"`
}
