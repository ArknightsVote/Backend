package logger

import (
	"os"
	"time"

	"github.com/natefinch/lumberjack"

	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
	"github.com/rs/zerolog/pkgerrors"
)

func Configure(level zerolog.Level) {
	zerolog.TimeFieldFormat = time.DateTime
	zerolog.DurationFieldUnit = time.Nanosecond
	zerolog.ErrorStackMarshaler = pkgerrors.MarshalStack

	file := &lumberjack.Logger{
		Filename:   "app.log",
		MaxSize:    10,
		MaxBackups: 3,
		MaxAge:     28,
		Compress:   true,
	}
	console := zerolog.ConsoleWriter{
		Out:        os.Stderr,
		TimeFormat: time.DateTime,
	}

	multiWriter := zerolog.MultiLevelWriter(console, file)

	log.Logger = zerolog.New(multiWriter).
		With().
		Timestamp().
		Caller().
		Logger().
		Level(level)
}
