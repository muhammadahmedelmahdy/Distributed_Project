# Distributed_Project
### Features:
- Login & Registration: Users can login or register with
their user ID and password. These credentials are then
saved in the server along with the user’s IP address, which
is dynamically retrieved upon login.
- Add Images: Users can add image to their directory using
the directory of services functionality which allows the
users to upload any images they have using the DOS
endpoints that are menioned in section V
- View Gallery: Users can view the gallery which allows
them to view all the images uploaded by other users
which enables them to see the images in low resolution
so that he can later request the image from this client or
user.
- Image Encryption & Decryption: We provide a
steganography functionality which allows the image to
be encrypted at the server side and only decrypted at
the recpient client side. Details of implementing this are
provided in the next sections to come.
- Access Rights: Access rights are stored in the DOS and
they are encrypted in the image metadata adding a second
layer of encryption so that when the image is decoded
at the recepient client, the number of access views are
limited to the access rights this client has.
- P2P Communication: Clients can communicate with
each other as peer to peer format which allows a client
to request an image or access rights to an image with
certain number of views from another client. The details
of this is provided in the next sections to come.
- Offline Support: We have a notifications functionality,
in case one of the clients misses a request because of
being offline, he can view the notifications of the requests
he missed and then decides to approve or reject them
accordingly.

### III. Performance

| Clients   | Time taken for a request (sec) | Time taken for 1000 requests (hr) |
|-----------|--------------------------------|------------------------------------|
| Client 1  | 9.33                           | 3.46                               |
| Client 2  | 9.14                           | 3.28                               |
| Client 3  | 11.25                          | 3.39                               |
