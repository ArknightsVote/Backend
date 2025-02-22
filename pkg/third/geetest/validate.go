package geetest

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
	"time"

	"github.com/rs/zerolog/log"

	strconv2 "github.com/savsgio/gotils/strconv"
)

var mainUrl = "https://gcaptcha4.geetest.com/validate"

// Validate 验证请求 token 是否有效，调用极验官方接口
func Validate(request map[string]string, userIP string, token string) bool {
	sign := hmacEncode(token, request["lot_number"])
	data := make(url.Values)
	data["lot_number"] = []string{request["lot_number"]}
	data["captcha_output"] = []string{request["captcha_output"]}
	data["pass_token"] = []string{request["pass_token"]}
	data["gen_time"] = []string{request["gen_time"]}
	data["captcha_id"] = []string{request["captcha_id"]}
	data["sign_token"] = []string{sign}

	cli := http.Client{Timeout: time.Second * 5}
	resp, err := cli.PostForm(mainUrl, data)
	if err != nil || resp.StatusCode != 200 {
		log.Warn().Err(err).Msg("极验服务器请求失败")
		return true
	}
	defer resp.Body.Close()
	var res response
	ret, _ := io.ReadAll(resp.Body)
	if err := json.Unmarshal(ret, &res); err != nil {
		log.Warn().Err(err).Msg("解析极验服务器响应失败")
		return true
	}
	if res.Status == "success" && res.Result == "success" {
		return true
	}
	log.Warn().Err(err).Any("res", res).Msg("异常用户访问：极验返回")
	return false
}

func hmacEncode(key string, data string) string {
	mac := hmac.New(sha256.New, strconv2.S2B(key))
	mac.Write(strconv2.S2B(data))
	return hex.EncodeToString(mac.Sum(nil))
}

type response struct {
	Status      string `json:"status"`
	Code        string `json:"code"`
	Msg         string `json:"msg"`
	Result      string `json:"result"`
	Reason      string `json:"reason"`
	CaptchaArgs struct {
		UsedType  string `json:"used_type"`
		UserIp    string `json:"user_ip"`
		LotNumber string `json:"lot_number"`
		Scene     string `json:"scene"`
		Referer   string `json:"referer"`
	} `json:"captcha_args"`
	Desc struct {
		Type string `json:"type"`
	} `json:"desc"`
}
