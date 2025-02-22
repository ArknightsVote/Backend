package v1

import (
	"typhon/api/v1/handlers"

	"github.com/gofiber/fiber/v2"
	"github.com/skadiD/database"
)

func SetupRoutes(app *fiber.App, db *database.Client) {
	api := app.Group("/api/v1")

	handlers.RegisterSystem(api.Group("/system"))
}
