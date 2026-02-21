package notifications

import (
	"context"
	"fmt"
	"log/slog"
)

// FCMSender sends push notifications via Firebase Cloud Messaging.
// Nil-safe: when not configured, all methods are no-ops.
type FCMSender struct {
	credentialsFile string
	logger          *slog.Logger
	// TODO: Add firebase.google.com/go/v4/messaging.Client when FCM
	// dependency is added. For now this is a structured placeholder
	// that logs send attempts.
}

// NewFCMSender creates an FCM sender from a service account credentials file.
// Returns nil if credentialsFile is empty (notifications disabled).
func NewFCMSender(credentialsFile string, logger *slog.Logger) *FCMSender {
	if credentialsFile == "" {
		return nil
	}
	return &FCMSender{
		credentialsFile: credentialsFile,
		logger:          logger,
	}
}

// SendMulti sends a notification to multiple device tokens.
// When the FCM client is integrated, this will call SendEachForMulticast.
// Currently logs the send for development/testing.
func (s *FCMSender) SendMulti(ctx context.Context, tokens []string, title, body string, data map[string]string) error {
	if s == nil {
		return nil // no-op when not configured
	}

	// TODO: Replace with actual FCM client call:
	//   msg := &messaging.MulticastMessage{
	//       Tokens:       tokens,
	//       Notification: &messaging.Notification{Title: title, Body: body},
	//       Data:         data,
	//   }
	//   resp, err := s.client.SendEachForMulticast(ctx, msg)

	s.logger.Info("FCM send (pending integration)",
		"tokens", len(tokens), "title", title, "body", body)

	if len(tokens) == 0 {
		return fmt.Errorf("no tokens to send to")
	}

	return nil
}
