package models

import (
	"github.com/skadiD/database/types"
	"net/netip"
)

type Vote struct {
	Id int `json:"id" db:"id" orm:"id,pk,auto"`
	// UserId 用户 ID
	UserId int `json:"user_id" db:"user_id" orm:"user_id"`
	// TopicId 投票主题 ID
	TopicId uint8 `json:"topic_id" db:"topic_id" orm:"topic_id"`
	// IP 当前投票 IP，外显不输出
	Ip netip.Addr `json:"-" db:"ip" orm:"ip"`
	// Selected 选中的选项
	Selected uint16 `json:"selected" db:"selected" orm:"selected"`
	// Lost 未选中的选项
	Lost uint16 `json:"lost" db:"lost" orm:"lost"`
	// Audit 投票状态
	Audit bool `json:"status" db:"status" orm:"status"`
	// CreatedAt 创建时间
	CreatedAt types.JsonTime `json:"created_at" db:"created_at" orm:"created_at"`
}
