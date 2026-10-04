// A field of the API's review that a view does not receive: it must not compile.
type Payload = { message: string };

Pinrail.connect<Payload>({
  onInit({ review }) {
    void review.origin;
  },
});
