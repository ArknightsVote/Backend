package models

import (
	"github.com/jackc/pgx/v5/pgtype"
	"github.com/skadiD/database/types"
	"net/netip"
)

type User struct {
	Id int `json:"id" db:"id" orm:"id,pk,auto"`
	// 对外显示 uid
	UID pgtype.UUID `json:"uuid" db:"uuid" orm:"uuid"`
	// 指纹
	SPM string `json:"spm" db:"spm" orm:"spm"`
	// 用户名
	UserName string `json:"username" db:"username" orm:"username"`
	// 密码
	Password string `json:"-" db:"password" orm:"password"`
	// 要存吗
	Ip netip.Addr `db:"ip" json:"-" orm:"ip"`
	// 头像
	Avatar string `db:"avatar" json:"avatar" orm:"avatar"`
	// 状态
	Status    UserStatus     `db:"status" json:"status" orm:"status"`
	CreatedAt types.JsonTime `db:"created_at" json:"created_at" orm:"created_at"`
	UpdatedAt types.JsonTime `db:"updated_at" json:"updated_at" orm:"updated_at"`
}
