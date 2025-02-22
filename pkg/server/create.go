package server

import (
	"github.com/goccy/go-json"
	"github.com/gofiber/contrib/fiberzerolog"
	"github.com/gofiber/fiber/v2"
	"github.com/gofiber/fiber/v2/middleware/cors"
	"github.com/gofiber/fiber/v2/middleware/recover"
	"github.com/rs/zerolog/log"
)

func NewFiber() *fiber.App {
	app := fiber.New(fiber.Config{
		ProxyHeader:  "X-Real-Ip",
		JSONEncoder:  json.Marshal,
		JSONDecoder:  json.Unmarshal,
		Network:      "tcp4",
		ServerHeader: "HuaWei",
		//Prefork:      true,
	})

	app.Use(recover.New())

	app.Use(fiberzerolog.New(fiberzerolog.Config{
		Logger: &log.Logger,
	}))

	app.Use(cors.New(cors.Config{
		AllowOrigins:     "*",
		AllowCredentials: false,
		AllowMethods:     "GET, POST, DELETE, PUT, OPTIONS",
		AllowHeaders:     "authorization, content-type, access-control-allow-origin, origin, x-request-id, apifoxtoken",
		MaxAge:           864000,
	}))

	return app
}
