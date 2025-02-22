package main

import (
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"syscall"
	"time"
	v1 "typhon/api/v1"
	"typhon/pkg/logger"
	"typhon/pkg/server"

	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/fiber/v2/middleware/limiter"
	"github.com/joho/godotenv"
	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
	"github.com/valyala/fasthttp/reuseport"
)

func main() {
	if err := godotenv.Load(); err != nil {
		fmt.Println("加载 .env 文件失败，请检查是否存在", err)
		os.Exit(1)
	}
	logger.Configure(zerolog.DebugLevel)

	app := server.NewFiber()
	app.Use(limiter.New(limiter.Config{
		Max:        20,
		Expiration: time.Second * 60,
	}))

	// db := database.NewClient(os.Getenv("APP_DB"))
	// TODO: 需要的时候再开
	v1.SetupRoutes(app, nil)

	run(app)
}

func run(app *fiber.App) {
	port := os.Getenv("APP_PORT")
	if os.Getenv("APP_BUILD_MODE") == "dev" {
		log.Info().Msg("开发模式已启用")
		log.Fatal().Err(app.Listen(port)).Send()
	} else {
		go func() {
			ln, err := reuseport.Listen("tcp4", port)
			if err != nil {
				log.Panic().Err(err).Msg("无法监听")
			}

			if err = app.Listener(ln); err != nil {
				log.Panic().Err(err).Msg("无法监听")
			}
		}()

		c := make(chan os.Signal, 1)
		signal.Notify(c, os.Interrupt, syscall.SIGHUP)
		<-c

		log.Info().Msg("正在热更新服务端...")
		exe, _ := os.Executable()
		cmd := exec.Command(exe)
		if err := cmd.Start(); err != nil {
			log.Error().Err(err).Msg("启动新端失败>_<")
			return
		}
		_ = app.Shutdown()
		log.Info().Msg("关闭数据库连接中...")
	}
}
