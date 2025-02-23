package models

type Admin struct {
	Id int `json:"id" db:"id" orm:"id,pk,auto"`
	// 用户名
	Username string `json:"username" db:"username" orm:"username"`
	// 密码
	Password string `json:"-" db:"password" orm:"password"`
	// 状态
	Status int16 `json:"status" db:"status" orm:"status"`
	// 管理员权限 enum
	//
	// AdminPermissionDefault AdminPermissionSuper
	Permission AdminPermission `json:"permission" db:"permission" orm:"permission"`
}
